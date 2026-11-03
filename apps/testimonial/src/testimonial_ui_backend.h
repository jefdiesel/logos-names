#pragma once

#include "rep_testimonial_ui_source.h"
#include "logos_ui_plugin_context.h"

#include <QNetworkAccessManager>

/// The Testimonial app's backend: plain `getAccountView` reads for the view, which has no
/// network of its own. It holds no key and calls no module: posting goes through the
/// Names Wallet Provider SDK from the QML (`propose_call`).
class TestimonialUiBackend : public TestimonialUiSimpleSource,
                                 public LogosUiPluginContext
{
public:
    QString accountView(QString sequencer, QString account, QString program) override;

private:
    QNetworkAccessManager* m_net = nullptr;
};
