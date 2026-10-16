#include "presentation/KeyedListModel.hpp"

#include <QSet>

namespace kodosi {

void KeyedListModel::setKey(const QString& key)
{
    if (m_key == key)
        return;
    m_key = key;
    emit keyChanged();
}

QString KeyedListModel::keyOf(const QVariant& value) const
{
    return value.toMap().value(m_key).toString();
}

void KeyedListModel::setValues(const QVariantList& next)
{
    if (next == m_values)
        return;
    QSet<QString> kept;
    for (const auto& value : next)
        kept.insert(keyOf(value));
    for (auto row = m_values.size() - 1; row >= 0; --row) {
        if (kept.contains(keyOf(m_values.at(row))))
            continue;
        beginRemoveRows({}, static_cast<int>(row), static_cast<int>(row));
        m_values.removeAt(row);
        endRemoveRows();
    }
    for (int row = 0; row < next.size(); ++row) {
        const auto key = keyOf(next.at(row));
        auto found = row;
        while (found < m_values.size() && keyOf(m_values.at(found)) != key)
            ++found;
        if (found == m_values.size()) {
            beginInsertRows({}, row, row);
            m_values.insert(row, next.at(row));
            endInsertRows();
            continue;
        }
        if (found != row) {
            beginMoveRows({}, static_cast<int>(found), static_cast<int>(found), {}, row);
            m_values.move(found, row);
            endMoveRows();
        }
        if (m_values.at(row) != next.at(row)) {
            m_values[row] = next.at(row);
            emit dataChanged(index(row), index(row));
        }
    }
    if (m_values.size() > next.size()) {
        beginRemoveRows({}, static_cast<int>(next.size()), static_cast<int>(m_values.size() - 1));
        m_values.resize(next.size());
        endRemoveRows();
    }
    emit valuesChanged();
}

int KeyedListModel::rowCount(const QModelIndex& parent) const
{
    return parent.isValid() ? 0 : static_cast<int>(m_values.size());
}

QVariant KeyedListModel::data(const QModelIndex& index, int role) const
{
    if (!index.isValid() || index.row() >= m_values.size() || role != Qt::UserRole)
        return {};
    return m_values.at(index.row());
}

QHash<int, QByteArray> KeyedListModel::roleNames() const
{
    return { { Qt::UserRole, QByteArrayLiteral("modelData") } };
}

}
