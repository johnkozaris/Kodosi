#pragma once

#include <QJsonObject>
#include <QPointer>
#include <QQuickItem>
#include <QTimer>
#include <QVariantMap>

#include <memory>
#include <optional>

namespace kodosi {
class VideoFrame;
class VideoStream;
class CanvasDocuments;
class CanvasModel;

// A view of a video pipe: the shared browser's or the screen's. Its frames are decoded on the
// runtime's thread of the pipe, so a busy GUI thread does not hold up the feedback that the host
// uses to keep the picture as sharp as this device and the link allow. The last picture of a view
// stays for the next view of the same feed, which shows it resting until the stream is live.
class StreamVideo : public QQuickItem {
    Q_OBJECT
    Q_PROPERTY(QObject* documents READ documents WRITE setDocuments FINAL)
    Q_PROPERTY(QObject* canvas READ canvas WRITE setCanvas FINAL)
    Q_PROPERTY(QString sessionId READ sessionId WRITE setSessionId FINAL)
    Q_PROPERTY(bool active READ active WRITE setActive FINAL)
    // The terminal's computer is offline, which the runtime waits out with the pipe.
    Q_PROPERTY(bool offline READ offline WRITE setOffline FINAL)
    // Why the computer shows nothing, as `away()` names it, or else "failed", "live", "resting" while
    // the last picture shows and the stream is not live, or "waiting" for a first picture.
    Q_PROPERTY(QString showing READ showing NOTIFY showingChanged FINAL)
    // Why the stream failed, as the runtime tells it.
    Q_PROPERTY(QString problem READ problem NOTIFY showingChanged FINAL)
    // How far the picture rests, from 0 for a live picture to 1 for one dimmed and greyed.
    Q_PROPERTY(qreal rest READ rest WRITE setRest NOTIFY restChanged FINAL)

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
    [[nodiscard]] bool offline() const { return m_offline; }
    void setOffline(bool offline);
    [[nodiscard]] QString showing() const;
    [[nodiscard]] QString problem() const { return m_problem; }
    [[nodiscard]] qreal rest() const { return m_rest; }
    void setRest(qreal rest);

    Q_INVOKABLE void pointer(const QString& action, qreal x, qreal y, int button, int clicks, int modifiers);
    Q_INVOKABLE void wheel(qreal x, qreal y, QPoint pixels, QPoint angle, int modifiers);
    Q_INVOKABLE bool key(int key, const QString& text, int modifiers, bool down);
    Q_INVOKABLE void paste();
    Q_INVOKABLE void step(const QVariantMap& input);
    // Opens the stream again, on a new pipe.
    Q_INVOKABLE void retry();

signals:
    void showingChanged();
    void restChanged();

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
    // Why the computer shows nothing, the reason that matters most first, or nothing.
    [[nodiscard]] virtual QString away() const;
    // Whether there is a stream to watch at all.
    [[nodiscard]] virtual bool watchable() const { return true; }
    // What the view leaves for the next view of its feed, besides the picture.
    [[nodiscard]] virtual QVariantMap kept() const { return {}; }
    // Takes up what the last view of the feed left.
    virtual void resume(const QVariantMap&) { }
    // The view turned to another terminal, so what the stream said is gone.
    virtual void forget() { }
    // A new pipe is a new seat at the stream, which starts as it is.
    virtual void seated() { }
    // The view's size settled, after a resize.
    virtual void settled() { }
    // The newest picture shows another part of the source than the one before.
    virtual void moved() { }
    // This viewer is about to act on the stream, which then follows whoever acted last.
    virtual void acting() { }

    void post(const QJsonObject& message);
    void join();
    void leave();
    // Closes the pipe while the view stays, so the decoder goes and the picture stays in memory.
    void pause();
    // The picture stopped being live though the pipe stays, until the next picture comes.
    void stale();
    // Leaves the picture and `kept()` for the next view of the feed. A view that keeps more than the
    // picture calls it from its own destructor, while `kept()` still answers for it.
    void keep();
    // Tells what the view shows again once something that it depends on changed.
    void restate();
    [[nodiscard]] bool watching() const { return m_watching; }
    [[nodiscard]] QSize pictureSize() const;
    // The part of the source that the newest picture shows, when only a part.
    [[nodiscard]] std::optional<QRectF> pictureRegion() const { return m_region; }

    QSGNode* updatePaintNode(QSGNode* node, UpdatePaintNodeData* data) override;
    void geometryChange(const QRectF& next, const QRectF& previous) override;
    void itemChange(ItemChange change, const ItemChangeData& value) override;
    void componentComplete() override;

private:
    void forgetTerminal();
    void takeUp();
    void chosen(const QString& sessionId, const QString& section);
    void opened(const QString& pipe);
    void watch();
    void received(quint64 generation, const QByteArray& message);
    void showPicture();
    void lost(const QString& pipe, const QString& reason);
    void closed(const QString& pipe, const QString& reason);
    void fellBehind();
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
    bool m_offline = false;
    bool m_watching = false;
    // Whether this view took up what the last view of its feed left, which it then leaves in turn.
    bool m_tookUp = false;
    std::shared_ptr<const VideoFrame> m_frame;
    std::optional<QRectF> m_region;
    // Whether the picture came on the open pipe, which has not been lost since.
    bool m_live = false;
    QTimer m_settle;
    QString m_problem;
    QString m_lastShowing;
    qreal m_rest = 0;
    QJsonObject m_sentSize;
    bool m_holding = false;
    std::shared_ptr<VideoStream> m_stream;
};
}
