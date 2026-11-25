#pragma once

#include <QImage>
#include <QJsonObject>
#include <QPointer>
#include <QQuickItem>
#include <QThread>
#include <QTimer>
#include <QVariantMap>

#include <optional>

namespace kodosi {
class VideoStream;
class CanvasDocuments;
class CanvasModel;

// A view of a video pipe: the shared browser's or the screen's. Decodes and acknowledges frames on a
// dedicated thread so a busy GUI thread does not build up a frame queue or stall the feedback the
// host uses to keep the picture as sharp as this device and the link allow.
class StreamVideo : public QQuickItem {
    Q_OBJECT
    Q_PROPERTY(QObject* documents READ documents WRITE setDocuments FINAL)
    Q_PROPERTY(QObject* canvas READ canvas WRITE setCanvas FINAL)
    Q_PROPERTY(QString sessionId READ sessionId WRITE setSessionId FINAL)
    Q_PROPERTY(bool active READ active WRITE setActive FINAL)
    Q_PROPERTY(bool shows READ shows NOTIFY showsChanged FINAL)
    Q_PROPERTY(bool reconnecting READ reconnecting NOTIFY reconnectingChanged FINAL)
    // Why the stream did not open, or went away, as the runtime tells it.
    Q_PROPERTY(QString problem READ problem NOTIFY problemChanged FINAL)

public:
    ~StreamVideo() override;

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
    [[nodiscard]] QString problem() const { return m_problem; }

    Q_INVOKABLE void pointer(const QString& action, qreal x, qreal y, int button, int clicks, int modifiers);
    Q_INVOKABLE void wheel(qreal x, qreal y, QPoint pixels, QPoint angle, int modifiers);
    Q_INVOKABLE bool key(int key, const QString& text, int modifiers, bool down);
    Q_INVOKABLE void paste();
    Q_INVOKABLE void step(const QVariantMap& input);
    // Opens the stream again, on a new pipe.
    Q_INVOKABLE void retry();

signals:
    void showsChanged();
    void reconnectingChanged();
    void problemChanged();

protected:
    // `key` names the pipe: the canvas's browser or screen.
    StreamVideo(const QString& key, QQuickItem* parent);

    // A message of the stream.
    virtual void take(const QJsonObject& message) = 0;
    // The stream's point under a view point, kept to the stream's edge when `clamped`, or nothing
    // outside the stream.
    [[nodiscard]] virtual std::optional<QPointF> streamPoint(QPointF point, bool clamped) const = 0;
    // Where the newest picture shows in the view.
    [[nodiscard]] virtual QRectF pictureRect() const;
    // Whether the stream plays this viewer's input.
    [[nodiscard]] virtual bool takesInput() const { return true; }
    // The view turned to another terminal, so what the stream said is gone.
    virtual void forget() { }
    // A new pipe is a new seat at the stream, which starts as it is.
    virtual void seated() { }
    // The stream heard this viewer's watch.
    virtual void watched() { }
    // The view's size settled, after a resize.
    virtual void settled() { }

    void post(const QJsonObject& message);
    [[nodiscard]] bool watching() const { return m_watching; }
    [[nodiscard]] QSize pictureSize() const { return m_image.size(); }
    // The part of the source that the newest picture shows, when only a part.
    [[nodiscard]] std::optional<QRectF> pictureRegion() const { return m_region; }

    QSGNode* updatePaintNode(QSGNode* node, UpdatePaintNodeData* data) override;
    void geometryChange(const QRectF& next, const QRectF& previous) override;
    void itemChange(ItemChange change, const ItemChangeData& value) override;

private:
    void join();
    void leave();
    void forgetTerminal();
    void chosen(const QString& sessionId, const QString& section);
    void opened(const QString& pipe);
    void watch();
    void received(const QString& pipe, quint64 generation, const QByteArray& message);
    void showPicture();
    void lost(const QString& pipe, const QString& reason);
    void closed(const QString& pipe, const QString& reason);
    void broke(const QString& pipe);
    void setReconnecting(bool reconnecting);
    void setProblem(const QString& problem);
    void input(const QJsonObject& event);
    void sendSize();
    void press(int key, const QString& typed, bool down, int modifiers);
    void type(const QString& text);
    [[nodiscard]] QJsonObject sizeMessage() const;

    QString m_key;
    QPointer<CanvasDocuments> m_documents;
    QPointer<CanvasModel> m_canvas;
    QString m_sessionId;
    QString m_pipe;
    bool m_active = false;
    bool m_watching = false;
    QImage m_image;
    std::optional<QRectF> m_region;
    bool m_fresh = false;
    QTimer m_settle;
    bool m_reconnecting = false;
    QString m_problem;
    QJsonObject m_sentSize;
    bool m_holding = false;
    QThread m_decoding;
    VideoStream* m_stream = nullptr;
};
}
