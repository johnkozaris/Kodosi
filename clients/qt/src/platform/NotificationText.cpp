#include "platform/NotificationText.hpp"

#include <QChar>

namespace kodosi {

QString notificationPlainText(
    const QStringView value,
    const qsizetype maximum)
{
    QString result;
    qsizetype count = 0;
    for (qsizetype offset = 0;
         offset < value.size() && count < maximum;
         ++count) {
        const auto first = value.at(offset);
        char32_t codePoint = first.unicode();
        if (first.isHighSurrogate() && offset + 1 < value.size()
            && value.at(offset + 1).isLowSurrogate()) {
            codePoint = QChar::surrogateToUcs4(first, value.at(offset + 1));
            offset += 2;
        } else {
            ++offset;
            if (first.isSurrogate()) {
                codePoint = QChar::ReplacementCharacter;
            }
        }
        const auto category = QChar::category(codePoint);
        if (category == QChar::Other_Control
            || category == QChar::Other_Format
            || category == QChar::Separator_Line
            || category == QChar::Separator_Paragraph) {
            result.append(QLatin1Char(' '));
        } else {
            result.append(QString::fromUcs4(&codePoint, 1));
        }
    }
    return result.trimmed();
}

}
