#include "testimonial_ui_backend.h"

#include <QEventLoop>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QNetworkReply>
#include <QNetworkRequest>
#include <QRegularExpression>
#include <QTimer>
#include <QUrl>

namespace {

/// How long one read may take, and the most a reply may hold. A testimonial is at most
/// about 1.3 KB, which a sequencer writes as a JSON array of numbers: well under this.
constexpr int kTimeoutMs = 15000;
constexpr qint64 kMaxReply = 256 * 1024;

QString answer(const QJsonObject& value)
{
    return QString::fromUtf8(QJsonDocument(value).toJson(QJsonDocument::Compact));
}

QString failure(const QString& why)
{
    return answer({{"ok", false}, {"error", why}});
}

bool isAccountId(const QString& text)
{
    static const QRegularExpression base58(QStringLiteral("^[1-9A-HJ-NP-Za-km-z]{32,44}$"));
    return base58.match(text).hasMatch();
}

} // namespace

QString TestimonialUiBackend::accountView(QString sequencer, QString account, QString program)
{
    const QUrl url(sequencer.trimmed(), QUrl::StrictMode);
    if (!url.isValid() || (url.scheme() != "http" && url.scheme() != "https") || url.host().isEmpty())
        return failure(QStringLiteral("the sequencer is not an http or https URL"));
    if (!isAccountId(account) || !isAccountId(program))
        return failure(QStringLiteral("not an account ID"));

    if (!m_net)
        m_net = new QNetworkAccessManager(this);

    const QJsonObject params{{"account_id", account}, {"program_account_id", program}};
    const QJsonObject body{{"jsonrpc", "2.0"}, {"id", 1}, {"method", "getAccountView"}, {"params", QJsonArray{params}}};
    QNetworkRequest request(url);
    request.setHeader(QNetworkRequest::ContentTypeHeader, QStringLiteral("application/json"));
    // A sequencer answers where it is asked; a redirect is refused, not followed.
    request.setAttribute(QNetworkRequest::RedirectPolicyAttribute, QNetworkRequest::ManualRedirectPolicy);
    request.setMaximumRedirectsAllowed(0);

    QNetworkReply* reply = m_net->post(request, QJsonDocument(body).toJson(QJsonDocument::Compact));
    QEventLoop wait;
    QTimer timer;
    timer.setSingleShot(true);
    QObject::connect(&timer, &QTimer::timeout, &wait, &QEventLoop::quit);
    QObject::connect(reply, &QNetworkReply::finished, &wait, &QEventLoop::quit);
    QObject::connect(reply, &QNetworkReply::downloadProgress, &wait, [&](qint64 received, qint64) {
        if (received > kMaxReply)
            reply->abort();
    });
    timer.start(kTimeoutMs);
    wait.exec();

    QString result;
    if (!reply->isFinished()) {
        reply->abort();
        result = failure(QStringLiteral("the sequencer did not answer in time"));
    } else if (reply->error() != QNetworkReply::NoError) {
        result = failure(QStringLiteral("could not reach the sequencer: ") + reply->errorString());
    } else {
        const QByteArray bytes = reply->read(kMaxReply + 1);
        QJsonParseError parseError;
        const QJsonDocument document = QJsonDocument::fromJson(bytes, &parseError);
        if (bytes.size() > kMaxReply)
            result = failure(QStringLiteral("the sequencer sent a reply larger than any account"));
        else if (parseError.error != QJsonParseError::NoError || !document.isObject())
            result = failure(QStringLiteral("the sequencer sent an unreadable reply"));
        else if (document.object().contains("error")) {
            const QJsonValue error = document.object().value("error");
            const QString said = error.isObject() ? error.toObject().value("message").toString() : error.toString();
            result = failure(QStringLiteral("the sequencer refused the read") + (said.isEmpty() ? QString() : QStringLiteral(": ") + said.left(200)));
        } else
            result = answer({{"ok", true}, {"result", document.object().value("result")}});
    }
    reply->deleteLater();
    return result;
}
