#include "canvas/BrowserVideo.hpp"
#include "presentation/CanvasDocuments.hpp"
#include "presentation/CanvasModel.hpp"

#include <kodosi_runtime.h>

#include <QClipboard>
#include <QGuiApplication>
#include <QJsonDocument>
#include <QQuickWindow>
#include <QSGSimpleTextureNode>
#include <QTimer>
#include <QtEndian>

#include <algorithm>

namespace kodosi {
namespace {
constexpr char messageTag = 0;
constexpr char frameTag = 1;
constexpr qsizetype frameHeader = 14;
constexpr int longestWait = 30000;
constexpr qsizetype mostTyped = 16 * 1024;

int browserModifiers(int modifiers)
{
    return (modifiers & Qt::AltModifier ? 1 : 0) | (modifiers & Qt::ControlModifier ? 2 : 0)
        | (modifiers & Qt::MetaModifier ? 4 : 0) | (modifiers & Qt::ShiftModifier ? 8 : 0);
}

// Decodes on its own thread. A frame depends on the frames before it, so each one is decoded.
class Decoder final : public QObject {
    Q_OBJECT

public:
    Decoder()
        : m_decoder(kodosi_video_decoder_new())
    {
    }
    ~Decoder() override { kodosi_video_decoder_free(m_decoder); }

public slots:
    void decode(quint32 frame, const QByteArray& data)
    {
        kodosi_picture_t picture {};
        const auto answer = kodosi_video_decoder_decode(
            m_decoder, reinterpret_cast<const uint8_t*>(data.constData()), static_cast<size_t>(data.size()), &picture);
        if (answer <= 0) {
            emit done(frame, {});
            return;
        }
        const QImage image(picture.rgba, static_cast<int>(picture.width), static_cast<int>(picture.height),
            static_cast<qsizetype>(picture.width) * 4, QImage::Format_RGBA8888);
        emit done(frame, image.copy());
    }

signals:
    void done(quint32 frame, const QImage& image);

private:
    kodosi_video_decoder_t* m_decoder;
};
}

BrowserVideo::BrowserVideo(QQuickItem* parent)
    : QQuickItem(parent)
{
    setFlag(ItemHasContents);
    auto* decoder = new Decoder;
    decoder->moveToThread(&m_decoding);
    connect(&m_decoding, &QThread::finished, decoder, &QObject::deleteLater);
    connect(this, &BrowserVideo::decodeRequested, decoder, &Decoder::decode);
    connect(decoder, &Decoder::done, this, &BrowserVideo::decoded);
    m_decoding.start();
    m_again.setSingleShot(true);
    connect(&m_again, &QTimer::timeout, this, &BrowserVideo::join);
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
    connect(m_documents, &CanvasDocuments::opened, this, &BrowserVideo::opened);
    connect(m_documents, &CanvasDocuments::receivedBytes, this, &BrowserVideo::received);
    connect(m_documents, &CanvasDocuments::lost, this, &BrowserVideo::lost);
    connect(m_documents, &CanvasDocuments::reachable, this, &BrowserVideo::reachable);
    join();
}

void BrowserVideo::setSessionId(const QString& sessionId)
{
    if (sessionId == m_sessionId)
        return;
    leave();
    m_sessionId = sessionId;
    join();
}

void BrowserVideo::setActive(bool active)
{
    if (active == m_active)
        return;
    m_active = active;
    if (active)
        join();
    else
        leave();
}

void BrowserVideo::join()
{
    if (!m_active || !m_documents || m_sessionId.isEmpty() || !m_pipe.isEmpty())
        return;
    m_pipe = m_documents->openBytes(m_sessionId, QStringLiteral("browser"));
    if (m_pipe.isEmpty())
        m_again.start(m_wait);
}

void BrowserVideo::leave()
{
    m_again.stop();
    if (m_documents && !m_pipe.isEmpty())
        m_documents->close(m_pipe);
    m_pipe.clear();
}

void BrowserVideo::reachable(const QString& sessionId)
{
    if (sessionId != m_sessionId)
        return;
    m_wait = 1000;
    if (m_again.isActive()) {
        m_again.stop();
        join();
    }
}

void BrowserVideo::opened(const QString& pipe)
{
    if (pipe != m_pipe)
        return;
    m_wait = 1000;
    m_behind = true;
    m_sentSize = size().toSize();
    post({ { QStringLiteral("type"), QStringLiteral("watch") }, { QStringLiteral("size"), sizeMessage() } });
}

void BrowserVideo::received(const QString& pipe, const QByteArray& message)
{
    if (pipe != m_pipe)
        return;
    if (message.startsWith(frameTag)) {
        this->frame(message);
    } else if (message.startsWith(messageTag)) {
        const auto state = QJsonDocument::fromJson(message.mid(1)).object();
        m_window = state.toVariantMap();
        emit windowChanged();
        const auto copied = state.value(QStringLiteral("copied")).toString();
        if (!copied.isEmpty())
            QGuiApplication::clipboard()->setText(copied);
    }
}

void BrowserVideo::lost(const QString& pipe)
{
    if (pipe != m_pipe)
        return;
    m_pipe.clear();
    m_again.start(m_wait);
    m_wait = std::min(m_wait * 2, longestWait);
}

void BrowserVideo::frame(const QByteArray& message)
{
    if (message.size() <= frameHeader)
        return;
    const auto id = qFromBigEndian<quint32>(message.constData() + 1);
    const bool key = message.at(5) != 0;
    if (!key && !m_behind && id != m_lastFrame + 1) {
        m_behind = true;
        post({ { QStringLiteral("type"), QStringLiteral("keyframe") } });
    }
    m_lastFrame = id;
    if (m_behind && !key) {
        post({ { QStringLiteral("type"), QStringLiteral("ack") }, { QStringLiteral("frame"), static_cast<qint64>(id) } });
        return;
    }
    m_behind = false;
    emit decodeRequested(id, message.mid(frameHeader));
}

void BrowserVideo::decoded(quint32 frame, const QImage& image)
{
    post({ { QStringLiteral("type"), QStringLiteral("ack") }, { QStringLiteral("frame"), static_cast<qint64>(frame) } });
    if (image.isNull())
        return;
    const bool first = m_image.isNull();
    m_image = image;
    m_fresh = true;
    update();
    if (first)
        emit showsChanged();
}

void BrowserVideo::post(const QJsonObject& message)
{
    if (!m_documents || m_pipe.isEmpty())
        return;
    QByteArray bytes(1, messageTag);
    bytes += QJsonDocument(message).toJson(QJsonDocument::Compact);
    m_documents->send(m_pipe, QString::fromLatin1(bytes.toBase64()));
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
}

void BrowserVideo::step(const QVariantMap& input)
{
    this->input(QJsonObject::fromVariantMap(input));
}

QSizeF BrowserVideo::browserSize() const
{
    return QSizeF(m_window.value(QStringLiteral("width")).toReal(), m_window.value(QStringLiteral("height")).toReal());
}

void BrowserVideo::pointer(const QString& action, qreal x, qreal y, int button, int clicks, int modifiers)
{
    const auto at = CanvasModel::browserPoint(QPointF(x, y), size(), browserSize());
    if (!at)
        return;
    const bool moving = action == QStringLiteral("move");
    if (!moving && size().toSize() != m_sentSize) {
        m_sentSize = size().toSize();
        post({ { QStringLiteral("type"), QStringLiteral("size") }, { QStringLiteral("size"), sizeMessage() } });
    }
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
    auto* texture = static_cast<QSGSimpleTextureNode*>(node);
    if (m_image.isNull()) {
        delete texture;
        return nullptr;
    }
    if (!texture) {
        texture = new QSGSimpleTextureNode;
        texture->setOwnsTexture(true);
        texture->setFiltering(QSGTexture::Linear);
    }
    if (m_fresh || !texture->texture()) {
        texture->setTexture(QQuickItem::window()->createTextureFromImage(m_image, QQuickWindow::TextureIsOpaque));
        m_fresh = false;
    }
    texture->setRect(pictureRect());
    return texture;
}

void BrowserVideo::geometryChange(const QRectF& next, const QRectF& previous)
{
    QQuickItem::geometryChange(next, previous);
    update();
}
}

#include "BrowserVideo.moc"
