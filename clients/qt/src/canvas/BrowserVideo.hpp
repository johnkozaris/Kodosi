#pragma once

#include <QImage>
#include <QJsonObject>
#include <QPointer>
#include <QQuickItem>
#include <QThread>
#include <QTimer>
#include <QVariantMap>

namespace kodosi {
class CanvasDocuments;

// The live picture of a terminal's shared browser, and a person's input to it. Each frame shows
// when it is decoded and is acknowledged then, so the host keeps the picture as sharp as this
// device and the link allow, and no queue forms.
class BrowserVideo : public QQuickItem {
    Q_OBJECT
    Q_PROPERTY(QObject* documents READ documents WRITE setDocuments)
    Q_PROPERTY(QString sessionId READ sessionId WRITE setSessionId)
    Q_PROPERTY(bool active READ active WRITE setActive)
    Q_PROPERTY(bool shows READ shows NOTIFY showsChanged)
    Q_PROPERTY(QVariantMap window READ window NOTIFY windowChanged)

public:
    explicit BrowserVideo(QQuickItem* parent = nullptr);
    ~BrowserVideo() override;

    [[nodiscard]] QObject* documents() const;
    void setDocuments(QObject* documents);
    [[nodiscard]] QString sessionId() const { return m_sessionId; }
    void setSessionId(const QString& sessionId);
    [[nodiscard]] bool active() const { return m_active; }
    void setActive(bool active);
    [[nodiscard]] bool shows() const { return !m_image.isNull(); }
    [[nodiscard]] QVariantMap window() const { return m_window; }

    Q_INVOKABLE void pointer(const QString& action, qreal x, qreal y, int button, int clicks, int modifiers);
    Q_INVOKABLE void wheel(qreal x, qreal y, QPoint pixels, QPoint angle, int modifiers);
    Q_INVOKABLE bool key(int key, const QString& text, int modifiers, bool down);
    Q_INVOKABLE void paste();
    Q_INVOKABLE void step(const QVariantMap& input);
    Q_INVOKABLE [[nodiscard]] QVariant viewPoint(qreal x, qreal y) const;

signals:
    void showsChanged();
    void windowChanged();
    void decodeRequested(quint32 frame, const QByteArray& data);

protected:
    QSGNode* updatePaintNode(QSGNode* node, UpdatePaintNodeData* data) override;
    void geometryChange(const QRectF& next, const QRectF& previous) override;

private:
    void join();
    void leave();
    void opened(const QString& pipe);
    void received(const QString& pipe, const QString& frame);
    void lost(const QString& pipe);
    void reachable(const QString& sessionId);
    void frame(const QByteArray& message);
    void decoded(quint32 frame, const QImage& image);
    void post(const QJsonObject& message);
    void input(const QJsonObject& event);
    void press(int key, const QString& typed, bool down, int modifiers);
    void type(const QString& text);
    [[nodiscard]] QJsonObject sizeMessage() const;
    [[nodiscard]] QSizeF browserSize() const;
    [[nodiscard]] QRectF pictureRect() const;

    QPointer<CanvasDocuments> m_documents;
    QString m_sessionId;
    QString m_pipe;
    bool m_active = false;
    QVariantMap m_window;
    QImage m_image;
    bool m_fresh = false;
    quint32 m_lastFrame = 0;
    bool m_behind = true;
    int m_wait = 1000;
    QTimer m_again;
    QSize m_sentSize;
    QThread m_decoding;
};
}
