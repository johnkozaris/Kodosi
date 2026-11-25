#pragma once

#include <QObject>
#include <QString>
#include <QStringView>
#include <QVariantList>

namespace kodosi {

// Converts agent Markdown into Qt rich text while HTML-escaping all raw text.
class ChatProse final : public QObject {
    Q_OBJECT

public:
    using QObject::QObject;

    // Splits Markdown into QML block maps (`lines`, `heading`, `code`, `table`). An unclosed code
    // fence consumes the rest of the text so streaming output renders cleanly. File paths in
    // `lines` blocks link to files resolved against `folder`, or to `~/` paths in the home folder.
    Q_INVOKABLE [[nodiscard]] static QVariantList blocks(const QString& text, const QString& folder = {});
    // Only https:// URLs are rendered as clickable links.
    [[nodiscard]] static QString html(QStringView text);
    Q_INVOKABLE [[nodiscard]] static QString text(const QString& text);
    [[nodiscard]] static QStringList cells(QStringView line);
};

}
