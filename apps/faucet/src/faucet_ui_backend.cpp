#include "faucet_ui_backend.h"

#include <QEventLoop>
#include <QHostAddress>
#include <QJsonDocument>
#include <QJsonObject>
#include <QNetworkReply>
#include <QNetworkRequest>
#include <QRegularExpression>
#include <QTimer>
#include <QUrl>

namespace {

/// The Names faucet answers once it has submitted the transfer (it confirms arrival in the
/// background), well within a minute; a busy public testnet can be slower.
constexpr int kTimeoutMs = 60000;
constexpr qint64 kMaxReply = 16 * 1024;

QString answer(const QJsonObject& value)
{
    return QString::fromUtf8(QJsonDocument(value).toJson(QJsonDocument::Compact));
}

QString unreachable(const QString& why)
{
    return answer({{"status", 0}, {"body", QJsonValue()}, {"retry_after", ""}, {"error", why}});
}

/// https anywhere; http only to this computer.
bool allowed(const QUrl& url)
{
    if (!url.isValid() || url.host().isEmpty() || !url.userInfo().isEmpty())
        return false;
    if (url.scheme() == "https")
        return true;
    if (url.scheme() != "http")
        return false;
    const QString host = url.host().toLower();
    const QHostAddress address(host);
    return host == "localhost" || (!address.isNull() && address.isLoopback());
}

} // namespace

QString FaucetUiBackend::exchange(const QString& text, const QByteArray* body)
{
    const QUrl url(text.trimmed(), QUrl::StrictMode);
    if (!allowed(url))
        return unreachable(QStringLiteral("the faucet address must be https (or http on this computer)"));
    if (!m_net)
        m_net = new QNetworkAccessManager(this);

    QNetworkRequest request(url);
    request.setAttribute(QNetworkRequest::RedirectPolicyAttribute, QNetworkRequest::ManualRedirectPolicy);
    request.setMaximumRedirectsAllowed(0);
    QNetworkReply* reply;
    if (body) {
        request.setHeader(QNetworkRequest::ContentTypeHeader, QStringLiteral("application/json"));
        reply = m_net->post(request, *body);
    } else {
        reply = m_net->get(request);
    }

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
    const int status = reply->attribute(QNetworkRequest::HttpStatusCodeAttribute).toInt();
    if (!reply->isFinished()) {
        reply->abort();
        result = unreachable(QStringLiteral("the faucet did not answer within a minute"));
    } else if (status == 0) {
        result = unreachable(reply->errorString());
    } else {
        // An HTTP error status still carries the faucet's reason in its body.
        const QByteArray bytes = reply->readAll().left(kMaxReply);
        const QJsonDocument document = QJsonDocument::fromJson(bytes);
        result = answer({
            {"status", status},
            {"body", document.isObject() ? QJsonValue(document.object()) : QJsonValue()},
            {"retry_after", QString::fromLatin1(reply->rawHeader("Retry-After")).left(64)},
            {"error", ""},
        });
    }
    reply->deleteLater();
    return result;
}

QString FaucetUiBackend::requestFunds(QString url, QString account)
{
    static const QRegularExpression base58(QStringLiteral("^[1-9A-HJ-NP-Za-km-z]{32,44}$"));
    const QString bare = account.trimmed().startsWith("Public/") ? account.trimmed().mid(7) : account.trimmed();
    if (!base58.match(bare).hasMatch())
        return unreachable(QStringLiteral("not a public account ID"));
    const QByteArray body = QJsonDocument(QJsonObject{{"account", bare}}).toJson(QJsonDocument::Compact);
    return exchange(url, &body);
}

QString FaucetUiBackend::describe(QString url)
{
    return exchange(url, nullptr);
}
