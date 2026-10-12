#pragma once

#include <QByteArrayView>
#include <QByteArray>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QSet>
#include <QString>
#include <QVector>
#include <charconv>
#include <cstdint>
#include <optional>

namespace kodosi {

struct JsonField {
    QString name;
    QByteArrayView value;
};

inline std::optional<QVector<JsonField>> jsonObjectFields(const QByteArray& json)
{
    QJsonParseError error;
    const auto document = QJsonDocument::fromJson(json, &error);
    if (error.error != QJsonParseError::NoError || !document.isObject()) return std::nullopt;
    if (document.object().size() > 256) return std::nullopt;
    const auto bytes = QByteArrayView(json).trimmed();
    if (!bytes.startsWith('{')) return std::nullopt;
    QVector<JsonField> fields;
    QSet<QString> names;
    qsizetype cursor = 1;
    const auto skipSpace = [&] {
        while (cursor < bytes.size() && (bytes[cursor] == ' ' || bytes[cursor] == '\n'
            || bytes[cursor] == '\r' || bytes[cursor] == '\t')) ++cursor;
    };
    while (cursor < bytes.size()) {
        skipSpace();
        if (cursor >= bytes.size() || bytes[cursor] == '}') break;
        if (bytes[cursor] != '"' || fields.size() >= 256) return std::nullopt;
        const auto keyStart = cursor++;
        while (cursor < bytes.size()) {
            if (bytes[cursor] == '\\') cursor += 2;
            else if (bytes[cursor++] == '"') break;
        }
        const auto key = QJsonDocument::fromJson('[' + bytes.sliced(keyStart, cursor - keyStart).toByteArray() + ']')
            .array().first().toString();
        if (names.contains(key)) return std::nullopt;
        names.insert(key);
        skipSpace();
        if (cursor >= bytes.size() || bytes[cursor] != ':') return std::nullopt;
        ++cursor;
        skipSpace();
        const auto valueStart = cursor;
        int depth = 0;
        bool quoted = false;
        while (cursor < bytes.size()) {
            const char character = bytes[cursor];
            if (quoted) {
                if (character == '\\') { cursor += 2; continue; }
                if (character == '"') quoted = false;
            } else if (character == '"') quoted = true;
            else if (character == '[' || character == '{') ++depth;
            else if (character == ']' || character == '}') {
                if (depth == 0) break;
                --depth;
            } else if (character == ',' && depth == 0) break;
            ++cursor;
        }
        fields.append({key, bytes.sliced(valueStart, cursor - valueStart).trimmed()});
        if (cursor < bytes.size() && bytes[cursor] == ',') ++cursor;
    }
    return fields;
}

inline std::optional<std::uint64_t> jsonUnsigned(QByteArrayView bytes)
{
    if (bytes.isEmpty()) return std::nullopt;
    std::uint64_t value = 0;
    const auto result = std::from_chars(bytes.data(), bytes.data() + bytes.size(), value);
    if (result.ec != std::errc {} || result.ptr != bytes.data() + bytes.size())
        return std::nullopt;
    return value;
}

inline QByteArray jsonStringBytes(const QString& value)
{
    const auto array = QJsonDocument(QJsonArray {value}).toJson(QJsonDocument::Compact);
    return array.mid(1, array.size() - 2);
}

}
