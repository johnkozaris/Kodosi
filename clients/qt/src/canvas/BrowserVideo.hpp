#pragma once

#include <QImage>
#include <QJsonArray>
#include <QJsonObject>
#include <QPointer>
#include <QQuickItem>
#include <QThread>
#include <QTimer>
#include <QVariantMap>

namespace kodosi {
class BrowserStream;
class CanvasDocuments;
class CanvasModel;

// Each frame is decoded and acknowledged on a thread of its own, so the host keeps the picture as
// sharp as this device and the link allow while the GUI thread is busy, and no queue forms.
class BrowserVideo : public QQuickItem {
    Q_OBJECT
    Q_PROPERTY(QObject* documents READ documents WRITE setDocuments FINAL)
    Q_PROPERTY(QObject* canvas READ canvas WRITE setCanvas FINAL)
    Q_PROPERTY(QString sessionId READ sessionId WRITE setSessionId FINAL)
    Q_PROPERTY(bool active READ active WRITE setActive FINAL)
    Q_PROPERTY(bool shows READ shows NOTIFY showsChanged FINAL)
    Q_PROPERTY(bool reconnecting READ reconnecting NOTIFY reconnectingChanged FINAL)
    Q_PROPERTY(QVariantMap window READ window NOTIFY windowChanged FINAL)
    Q_PROPERTY(QVariantMap icons READ icons NOTIFY iconsChanged FINAL)

public:
    explicit BrowserVideo(QQuickItem* parent = nullptr);
    ~BrowserVideo() override;

    [[nodiscard]] QObject* documents() const;
    void setDocuments(QObject* documents);
    [[nodiscard]] QObject* canvas() const;
    void setCanvas(QObject* canvas);
    [[nodiscard]] QString sessionId() const { return m_sessionId; }
    void setSessionId(const QString& sessionId);
    [[nodiscard]] bool active() const { return m_active; }
    void setActive(bool active);
    [[nodiscard]] bool shows() const { return !m_image.isNull(); }
    [[nodiscard]] bool reconnecting() const { return m_reconnecting; }
    [[nodiscard]] QVariantMap window() const { return m_window; }
    [[nodiscard]] QVariantMap icons() const { return m_icons; }

    Q_INVOKABLE void pointer(const QString& action, qreal x, qreal y, int button, int clicks, int modifiers);
    Q_INVOKABLE void wheel(qreal x, qreal y, QPoint pixels, QPoint angle, int modifiers);
    Q_INVOKABLE bool key(int key, const QString& text, int modifiers, bool down);
    Q_INVOKABLE void paste();
    Q_INVOKABLE void step(const QVariantMap& input);
    Q_INVOKABLE [[nodiscard]] QVariant viewPoint(qreal x, qreal y) const;

signals:
    void showsChanged();
    void reconnectingChanged();
    void windowChanged();
    void iconsChanged();

protected:
    QSGNode* updatePaintNode(QSGNode* node, UpdatePaintNodeData* data) override;
    void geometryChange(const QRectF& next, const QRectF& previous) override;
    void itemChange(ItemChange change, const ItemChangeData& value) override;

private:
    void join();
    void leave();
    void forgetTerminal();
    void chosen(const QString& sessionId);
    void opened(const QString& pipe);
    void watch();
    void received(const QString& pipe, quint64 generation, const QByteArray& message);
    void keepIcons(const QJsonObject& changed, const QJsonArray& tabs);
    void showPicture();
    void lost(const QString& pipe);
    void closed(const QString& pipe);
    void broke(const QString& pipe);
    void setReconnecting(bool reconnecting);
    void post(const QJsonObject& message);
    void input(const QJsonObject& event);
    void sendSize();
    void press(int key, const QString& typed, bool down, int modifiers);
    void type(const QString& text);
    [[nodiscard]] QJsonObject sizeMessage() const;
    [[nodiscard]] QSizeF browserSize() const;
    [[nodiscard]] QRectF pictureRect() const;

    QPointer<CanvasDocuments> m_documents;
    QPointer<CanvasModel> m_canvas;
    QString m_sessionId;
    QString m_pipe;
    bool m_active = false;
    bool m_watching = false;
    QVariantMap m_window;
    QVariantMap m_icons;
    QImage m_image;
    bool m_fresh = false;
    QTimer m_settle;
    bool m_reconnecting = false;
    QJsonObject m_sentSize;
    bool m_holding = false;
    QThread m_decoding;
    BrowserStream* m_stream = nullptr;
};
}
