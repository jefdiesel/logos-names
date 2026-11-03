#pragma once

#include "rep_faucet_ui_source.h"
#include "logos_ui_plugin_context.h"

#include <QNetworkAccessManager>

/// The Faucet app's backend: one HTTP call to the faucet service per request, for the view,
/// which has no network of its own. It holds no key and calls no module; the account to
/// fund comes from the Names Wallet Provider SDK (`granted_accounts`) in the QML.
class FaucetUiBackend : public FaucetUiSimpleSource,
                        public LogosUiPluginContext
{
public:
    QString requestFunds(QString url, QString account) override;
    QString describe(QString url) override;

private:
    QString exchange(const QString& url, const QByteArray* body);
    QNetworkAccessManager* m_net = nullptr;
};
