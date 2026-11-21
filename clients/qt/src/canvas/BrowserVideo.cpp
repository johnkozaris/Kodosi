#include "canvas/BrowserVideo.hpp"
#include "presentation/CanvasDocuments.hpp"
#include "presentation/CanvasModel.hpp"

#include <kodosi_runtime.h>

#include <QClipboard>
#include <QGuiApplication>
#include <QJsonArray>
#include <QJsonDocument>
#include <QQuickWindow>
#include <QSGImageNode>
#include <QSet>
#include <QTimer>
#include <QtEndian>

#include <algorithm>
#include <atomic>
#include <mutex>
#include <utility>

namespace kodosi {
namespace {
constexpr char messageTag = 0;
constexpr char frameTag = 1;
constexpr qsizetype frameHeader = 6;
constexpr char keyFlag = 1;
constexpr int settleWait = 250;
constexpr qsizetype mostTyped = 16 * 1024;
constexpr int mostWaitingStates = 64;
constexpr qsizetype mostWaitingStateBytes = 8 * 1024 * 1024;

int browserModifiers(int modifiers)
{
    return (modifiers & Qt::AltModifier ? 1 : 0) | (modifiers & Qt::ControlModifier ? 2 : 0)
        | (modifiers & Qt::MetaModifier ? 4 : 0) | (modifiers & Qt::ShiftModifier ? 8 : 0);
}
}

// Reads the browser's pipe on its own thread. A frame depends on the frames before it, so each one
// is decoded, and after a broken frame or on a new pipe only a key frame can begin again.
class BrowserStream final : public QObject {
    Q_OBJECT

public:
    ~BrowserStream() override { kodosi_video_decoder_free(m_decoder); }

    void read(const QByteArray& payload, bool afterDrop, CommandDispatcher& commands)
    {
        const auto event = QJsonDocument::fromJson(payload).object();
        const auto pipe = event.value(QStringLiteral("pipe")).toString();
        if (pipe.isEmpty())
            return;
        if (pipe != m_pipe) {
            m_pipe = pipe;
            m_parts.close();
            reset();
        }
        if (afterDrop)
            m_parts.close();
        const auto type = event.value(QStringLiteral("type")).toString();
        if (type == QStringLiteral("document.opened")) {
            m_parts.open();
            m_behind = true;
            m_broken = false;
            m_statesDropped = false;
            emit opened(pipe);
        } else if (type == QStringLiteral("document.lost")) {
            m_parts.lose();
            emit lost(pipe);
        } else if (type == QStringLiteral("document.closed")) {
            m_parts.close();
            emit closed(pipe);
        } else if (type == QStringLiteral("document.frame")) {
            const auto taken = m_parts.take(event);
            if (taken.broken)
                breakPipe();
            else if (taken.frame && taken.frame->startsWith(frameTag))
                frame(*taken.frame, commands);
            else if (taken.frame && taken.frame->startsWith(messageTag))
                hand(*taken.frame);
        }
    }

    // Any thread: the runtime takes the parts of one frame of a pipe in a row.
    bool post(CommandDispatcher& commands, const QString& pipe, const QJsonObject& message)
    {
        QByteArray bytes(1, messageTag);
        bytes += QJsonDocument(message).toJson(QJsonDocument::Compact);
        std::lock_guard lock(m_sending);
        return sendPipeFrame(commands, pipe, QString::fromLatin1(bytes.toBase64()));
    }

    // A picture that a busy GUI thread missed shows nothing that the newest does not.
    std::pair<QString, QImage> takePicture()
    {
        std::lock_guard lock(m_showing);
        return { std::exchange(m_picturePipe, {}), std::exchange(m_picture, {}) };
    }

    bool tookState(quint64 generation, qsizetype bytes)
    {
        m_waitingStates.fetch_sub(1);
        m_waitingStateBytes.fetch_sub(bytes);
        return generation == m_stateGeneration.load();
    }

signals:
    void opened(const QString& pipe);
    void received(const QString& pipe, quint64 generation, const QByteArray& message);
    void pictureReady();
    void lost(const QString& pipe);
    void closed(const QString& pipe);
    void broke(const QString& pipe);

private:
    void frame(const QByteArray& message, CommandDispatcher& commands)
    {
        if (message.size() <= frameHeader)
            return;
        const auto id = qFromBigEndian<quint32>(message.constData() + 1);
        const bool key = (message.at(5) & keyFlag) != 0;
        if (!key && !m_behind && id != m_lastFrame + 1) {
            m_behind = true;
            askKeyframe(commands);
        }
        m_lastFrame = id;
        if (m_behind && !key) {
            ack(commands, id);
            return;
        }
        m_behind = false;
        if (key)
            m_keyFrame = id;
        decode(commands, id, key, message.mid(frameHeader));
    }

    void decode(CommandDispatcher& commands, quint32 frame, bool key, const QByteArray& data)
    {
        if (!m_decoder && key)
            m_decoder = kodosi_video_decoder_new();
        if (!m_decoder) {
            if (key)
                decodeFailed(commands, frame);
            else
                ack(commands, frame);
            return;
        }
        kodosi_picture_t picture {};
        const auto answer = kodosi_video_decoder_decode(
            m_decoder, reinterpret_cast<const uint8_t*>(data.constData()), static_cast<size_t>(data.size()), &picture);
        if (answer < 0) {
            reset();
            decodeFailed(commands, frame);
            return;
        }
        if (answer > 0) {
            const QImage image(picture.rgba, static_cast<int>(picture.width), static_cast<int>(picture.height),
                static_cast<qsizetype>(picture.width) * 4, QImage::Format_RGBA8888);
            show(image.copy());
        }
        ack(commands, frame);
    }

    // The decoder fails each frame after a broken one, so the view waits for a key frame after the
    // first failure. A frame that the decoder had from before the newest key frame broke nothing
    // that the key frame does not mend, and an ask for it would make the host send another.
    void decodeFailed(CommandDispatcher& commands, quint32 frame)
    {
        ack(commands, frame);
        if (m_behind || static_cast<qint32>(frame - m_keyFrame) < 0)
            return;
        m_behind = true;
        askKeyframe(commands);
    }

    // The host sends key frames at most once a second, as each costs every viewer.
    void askKeyframe(CommandDispatcher& commands)
    {
        if (!post(commands, m_pipe, { { QStringLiteral("type"), QStringLiteral("keyframe") } }))
            breakPipe();
    }

    void ack(CommandDispatcher& commands, quint32 frame)
    {
        if (!post(commands, m_pipe,
                { { QStringLiteral("type"), QStringLiteral("ack") },
                    { QStringLiteral("frame"), static_cast<qint64>(frame) } }))
            breakPipe();
    }

    void show(const QImage& image)
    {
        bool waking = false;
        {
            std::lock_guard lock(m_showing);
            waking = m_picture.isNull();
            m_picture = image;
            m_picturePipe = m_pipe;
        }
        if (waking)
            emit pictureReady();
    }

    // The states that wait for a busy GUI thread stay within a bound. Past it they go nowhere until
    // the pipe opens again, which brings the whole state.
    void hand(const QByteArray& message)
    {
        if (m_statesDropped)
            return;
        if (const auto waiting = m_waitingStates.load(); waiting >= mostWaitingStates
            || (waiting > 0 && m_waitingStateBytes.load() > mostWaitingStateBytes - message.size())) {
            m_statesDropped = true;
            m_stateGeneration.fetch_add(1);
            breakPipe();
            return;
        }
        m_waitingStates.fetch_add(1);
        m_waitingStateBytes.fetch_add(message.size());
        emit received(m_pipe, m_stateGeneration.load(), message);
    }

    // A part that went missing breaks the frames, and so does one that could not go, so the pipe
    // starts again.
    void breakPipe()
    {
        m_parts.close();
        if (!std::exchange(m_broken, true))
            emit broke(m_pipe);
    }

    void reset()
    {
        kodosi_video_decoder_free(m_decoder);
        m_decoder = nullptr;
    }

    QString m_pipe;
    PipeParts m_parts;
    kodosi_video_decoder_t* m_decoder = nullptr;
    quint32 m_lastFrame = 0;
    quint32 m_keyFrame = 0;
    bool m_behind = true;
    bool m_broken = false;
    bool m_statesDropped = false;
    std::mutex m_sending;
    std::mutex m_showing;
    QString m_picturePipe;
    QImage m_picture;
    std::atomic<int> m_waitingStates { 0 };
    std::atomic<qsizetype> m_waitingStateBytes { 0 };
    std::atomic<quint64> m_stateGeneration { 0 };
};

BrowserVideo::BrowserVideo(QQuickItem* parent)
    : QQuickItem(parent)
    , m_stream(new BrowserStream)
{
    setFlag(ItemHasContents);
    m_stream->moveToThread(&m_decoding);
    connect(&m_decoding, &QThread::finished, m_stream, &QObject::deleteLater);
    connect(m_stream, &BrowserStream::opened, this, &BrowserVideo::opened);
    connect(m_stream, &BrowserStream::received, this, &BrowserVideo::received);
    connect(m_stream, &BrowserStream::pictureReady, this, &BrowserVideo::showPicture);
    connect(m_stream, &BrowserStream::lost, this, &BrowserVideo::lost);
    connect(m_stream, &BrowserStream::closed, this, &BrowserVideo::closed);
    connect(m_stream, &BrowserStream::broke, this, &BrowserVideo::broke);
    m_decoding.start();
    m_settle.setSingleShot(true);
    m_settle.setInterval(settleWait);
    connect(&m_settle, &QTimer::timeout, this, &BrowserVideo::sendSize);
}

BrowserVideo::~BrowserVideo()
{
    leave();
    m_decoding.quit();
    m_decoding.wait();
}

QObject* BrowserVideo::documents() const
{
    return m_documents;
}

void BrowserVideo::setDocuments(QObject* documents)
{
    m_documents = qobject_cast<CanvasDocuments*>(documents);
    if (!m_documents)
        return;
    connect(m_documents, &CanvasDocuments::lost, this, &BrowserVideo::lost);
    connect(m_documents, &CanvasDocuments::closed, this, &BrowserVideo::closed);
    join();
}

void BrowserVideo::setSessionId(const QString& sessionId)
{
    if (sessionId == m_sessionId)
        return;
    leave();
    m_sessionId = sessionId;
    m_opening = true;
    forgetTerminal();
    join();
}

void BrowserVideo::forgetTerminal()
{
    setReconnecting(false);
    if (!m_image.isNull()) {
        m_image = {};
        update();
        emit showsChanged();
    }
    if (!m_window.isEmpty()) {
        m_window.clear();
        emit windowChanged();
    }
    if (!m_icons.isEmpty()) {
        m_icons.clear();
        emit iconsChanged();
    }
}

void BrowserVideo::setActive(bool active)
{
    if (active == m_active)
        return;
    m_active = active;
    if (active) {
        m_opening = true;
        join();
    } else {
        leave();
    }
}

void BrowserVideo::join()
{
    if (!m_active || !m_documents || m_sessionId.isEmpty() || !m_pipe.isEmpty())
        return;
    auto* stream = m_stream;
    auto& commands = m_documents->commands();
    m_pipe = m_documents->openReader(m_sessionId, QStringLiteral("browser"), *m_stream,
        [stream, &commands](const QByteArray& event, bool afterDrop) { stream->read(event, afterDrop, commands); });
}

void BrowserVideo::leave()
{
    if (m_documents && !m_pipe.isEmpty())
        m_documents->close(m_pipe);
    m_pipe.clear();
}

void BrowserVideo::opened(const QString& pipe)
{
    if (pipe != m_pipe)
        return;
    m_holding = false;
    m_sentSize = sizeMessage();
    post({ { QStringLiteral("type"), QStringLiteral("watch") },
        { QStringLiteral("size"), m_sentSize },
        { QStringLiteral("opened"), std::exchange(m_opening, false) } });
}

void BrowserVideo::received(const QString& pipe, quint64 generation, const QByteArray& message)
{
    if (!m_stream->tookState(generation, message.size()) || pipe != m_pipe)
        return;
    const auto state = QJsonDocument::fromJson(message.mid(1)).object();
    m_window = state.toVariantMap();
    emit windowChanged();
    keepIcons(state.value(QStringLiteral("icons")).toObject(), state.value(QStringLiteral("tabs")).toArray());
    const auto copied = state.value(QStringLiteral("copied")).toString();
    if (!copied.isEmpty())
        QGuiApplication::clipboard()->setText(copied);
}

void BrowserVideo::keepIcons(const QJsonObject& changed, const QJsonArray& tabs)
{
    auto kept = m_icons;
    for (auto icon = changed.begin(); icon != changed.end(); ++icon) {
        const auto png = icon.value().toString();
        if (png.isEmpty())
            kept.remove(icon.key());
        else
            kept.insert(icon.key(), QStringLiteral("data:image/png;base64,") + png);
    }
    QSet<QString> open;
    for (const auto& tab : tabs)
        open.insert(tab.toObject().value(QStringLiteral("id")).toString());
    for (auto icon = kept.begin(); icon != kept.end();)
        icon = open.contains(icon.key()) ? std::next(icon) : kept.erase(icon);
    if (kept != m_icons) {
        m_icons = kept;
        emit iconsChanged();
    }
}

void BrowserVideo::showPicture()
{
    const auto [pipe, image] = m_stream->takePicture();
    if (pipe != m_pipe || image.isNull())
        return;
    const bool first = m_image.isNull();
    m_image = image;
    m_fresh = true;
    update();
    setReconnecting(false);
    if (first)
        emit showsChanged();
}

void BrowserVideo::lost(const QString& pipe)
{
    if (pipe == m_pipe)
        setReconnecting(true);
}

void BrowserVideo::closed(const QString& pipe)
{
    if (pipe != m_pipe)
        return;
    if (m_documents)
        m_documents->forget(pipe);
    m_pipe.clear();
}

void BrowserVideo::broke(const QString& pipe)
{
    if (pipe == m_pipe && m_documents)
        m_documents->renew(pipe);
}

void BrowserVideo::setReconnecting(bool reconnecting)
{
    if (reconnecting == m_reconnecting)
        return;
    m_reconnecting = reconnecting;
    emit reconnectingChanged();
}

void BrowserVideo::post(const QJsonObject& message)
{
    if (!m_documents || m_pipe.isEmpty())
        return;
    if (!m_stream->post(m_documents->commands(), m_pipe, message))
        m_documents->renew(m_pipe);
}

QJsonObject BrowserVideo::sizeMessage() const
{
    const auto scale = QQuickItem::window() ? QQuickItem::window()->devicePixelRatio() : 1.0;
    return { { QStringLiteral("width"), qRound(width()) }, { QStringLiteral("height"), qRound(height()) },
        { QStringLiteral("scale"), scale } };
}

void BrowserVideo::input(const QJsonObject& event)
{
    post({ { QStringLiteral("type"), QStringLiteral("input") }, { QStringLiteral("input"), event } });
    if (event.value(QStringLiteral("kind")).toString() == QStringLiteral("mouse")) {
        const auto action = event.value(QStringLiteral("action")).toString();
        if (action == QStringLiteral("move") && event.value(QStringLiteral("button")).toString() == QStringLiteral("none"))
            return;
        if (action == QStringLiteral("press"))
            m_holding = true;
        else if (action == QStringLiteral("release"))
            m_holding = false;
    }
    sendSize();
}

// The page takes the shape of the view of the person who acted last, so a new size goes to the
// host once the view settles, never while a button is held, and one that waits goes right after
// the person's act: the act then reaches the page that the person saw.
void BrowserVideo::sendSize()
{
    const auto size = sizeMessage();
    if (m_holding || size == m_sentSize)
        return;
    m_sentSize = size;
    post({ { QStringLiteral("type"), QStringLiteral("size") }, { QStringLiteral("size"), size } });
}

void BrowserVideo::step(const QVariantMap& input)
{
    this->input(QJsonObject::fromVariantMap(input));
}

QSizeF BrowserVideo::browserSize() const
{
    return QSizeF(m_window.value(QStringLiteral("width")).toReal(), m_window.value(QStringLiteral("height")).toReal());
}

// While a button is held, a point beyond the page goes to its nearest edge, so that the page also
// gets the rest of the drag and its release.
void BrowserVideo::pointer(const QString& action, qreal x, qreal y, int button, int clicks, int modifiers)
{
    const auto at = CanvasModel::browserPoint(QPointF(x, y), size(), browserSize(), m_holding);
    if (!at)
        return;
    const bool moving = action == QStringLiteral("move");
    const auto name = button == Qt::LeftButton ? QStringLiteral("left")
        : button == Qt::RightButton            ? QStringLiteral("right")
        : button == Qt::MiddleButton           ? QStringLiteral("middle")
                                               : QStringLiteral("none");
    input({ { QStringLiteral("kind"), QStringLiteral("mouse") }, { QStringLiteral("action"), action },
        { QStringLiteral("x"), at->x() }, { QStringLiteral("y"), at->y() }, { QStringLiteral("button"), name },
        { QStringLiteral("clicks"), moving ? 0 : std::clamp(clicks, 0, 3) },
        { QStringLiteral("modifiers"), browserModifiers(modifiers) } });
}

void BrowserVideo::wheel(qreal x, qreal y, QPoint pixels, QPoint angle, int modifiers)
{
    const auto at = CanvasModel::browserPoint(QPointF(x, y), size(), browserSize());
    if (!at)
        return;
    const auto delta = pixels.isNull() ? angle * 100 / 120 : pixels;
    input({ { QStringLiteral("kind"), QStringLiteral("mouse") }, { QStringLiteral("action"), QStringLiteral("wheel") },
        { QStringLiteral("x"), at->x() }, { QStringLiteral("y"), at->y() }, { QStringLiteral("deltaX"), -delta.x() },
        { QStringLiteral("deltaY"), -delta.y() }, { QStringLiteral("modifiers"), browserModifiers(modifiers) } });
}

bool BrowserVideo::key(int key, const QString& text, int modifiers, bool down)
{
    const bool command = modifiers & (Qt::ControlModifier | Qt::AltModifier | Qt::MetaModifier);
    const auto printable = !text.isEmpty() && std::ranges::all_of(text, [](QChar character) { return character.isPrint(); });
    if (!CanvasModel::browserKey(key)) {
        if (down && printable && !command)
            type(text);
        return down && printable && !command;
    }
    QString typed = printable ? text : QString {};
    if (key >= Qt::Key_A && key <= Qt::Key_Z && !printable)
        typed = modifiers & Qt::ShiftModifier ? QChar(u'A' + (key - Qt::Key_A)) : QChar(u'a' + (key - Qt::Key_A));
    else if (key >= Qt::Key_0 && key <= Qt::Key_9 && !printable)
        typed = QChar(u'0' + (key - Qt::Key_0));
    press(key, typed, down, modifiers);
    return true;
}

void BrowserVideo::press(int key, const QString& typed, bool down, int modifiers)
{
    const auto found = CanvasModel::browserKey(key);
    const bool command = modifiers & (Qt::ControlModifier | Qt::AltModifier | Qt::MetaModifier);
    QString text;
    if (down && found->code == QStringLiteral("Space"))
        text = QStringLiteral(" ");
    else if (down && found->named == QStringLiteral("Enter"))
        text = QStringLiteral("\r");
    else if (down && found->named.isEmpty() && !command)
        text = typed;
    QJsonObject event { { QStringLiteral("kind"), QStringLiteral("key") },
        { QStringLiteral("action"), down ? QStringLiteral("down") : QStringLiteral("up") },
        { QStringLiteral("key"), found->named.isEmpty() ? typed : found->named }, { QStringLiteral("code"), found->code },
        { QStringLiteral("keyCode"), found->windows }, { QStringLiteral("modifiers"), browserModifiers(modifiers) } };
    if (!text.isEmpty())
        event.insert(QStringLiteral("text"), text);
    input(event);
}

void BrowserVideo::type(const QString& text)
{
    if (!text.isEmpty())
        input({ { QStringLiteral("kind"), QStringLiteral("text") }, { QStringLiteral("text"), text.left(mostTyped) } });
}

void BrowserVideo::paste()
{
    type(QGuiApplication::clipboard()->text());
}

QVariant BrowserVideo::viewPoint(qreal x, qreal y) const
{
    const auto at = CanvasModel::pointInView(QPointF(x, y), size(), browserSize());
    return at ? QVariant(*at) : QVariant {};
}

QRectF BrowserVideo::pictureRect() const
{
    const auto picture = QSizeF(m_image.size()).scaled(size(), Qt::KeepAspectRatio);
    return QRectF(QPointF((width() - picture.width()) / 2, (height() - picture.height()) / 2), picture);
}

QSGNode* BrowserVideo::updatePaintNode(QSGNode* node, UpdatePaintNodeData*)
{
    auto* picture = static_cast<QSGImageNode*>(node);
    if (m_image.isNull()) {
        delete picture;
        return nullptr;
    }
    if (!picture) {
        picture = QQuickItem::window()->createImageNode();
        picture->setOwnsTexture(true);
        picture->setFiltering(QSGTexture::Linear);
    }
    const auto shown = pictureRect();
    // Linear filtering alone skips pixels of a picture larger than the view, which makes it
    // shimmer. A texture keeps no copy of its picture, so other mipmaps need a new texture.
    const auto mipmaps = qRound(shown.width() * QQuickItem::window()->effectiveDevicePixelRatio()) < m_image.width()
        ? QSGTexture::Linear
        : QSGTexture::None;
    if (m_fresh || !picture->texture() || picture->mipmapFiltering() != mipmaps) {
        picture->setMipmapFiltering(mipmaps);
        picture->setTexture(QQuickItem::window()->createTextureFromImage(m_image, QQuickWindow::TextureIsOpaque));
        m_fresh = false;
    }
    picture->setRect(shown);
    return picture;
}

void BrowserVideo::geometryChange(const QRectF& next, const QRectF& previous)
{
    QQuickItem::geometryChange(next, previous);
    if (next.size() != previous.size())
        m_settle.start();
    update();
}

void BrowserVideo::itemChange(ItemChange change, const ItemChangeData& value)
{
    QQuickItem::itemChange(change, value);
    if (change == ItemDevicePixelRatioHasChanged) {
        m_settle.start();
        update();
    }
}
}

#include "BrowserVideo.moc"
