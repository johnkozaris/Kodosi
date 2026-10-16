#pragma once

#include <QAbstractListModel>
#include <QVariantList>

namespace kodosi {

class KeyedListModel : public QAbstractListModel {
    Q_OBJECT
    Q_PROPERTY(QString key READ key WRITE setKey NOTIFY keyChanged)
    Q_PROPERTY(QVariantList values READ values WRITE setValues NOTIFY valuesChanged)
    Q_PROPERTY(int count READ rowCount NOTIFY valuesChanged)
public:
    using QAbstractListModel::QAbstractListModel;

    [[nodiscard]] QString key() const { return m_key; }
    void setKey(const QString& key);
    [[nodiscard]] QVariantList values() const { return m_values; }
    void setValues(const QVariantList& values);

    [[nodiscard]] int rowCount(const QModelIndex& parent = {}) const override;
    [[nodiscard]] QVariant data(const QModelIndex& index, int role) const override;
    [[nodiscard]] QHash<int, QByteArray> roleNames() const override;

signals:
    void keyChanged();
    void valuesChanged();

private:
    [[nodiscard]] QString keyOf(const QVariant& value) const;

    QString m_key = QStringLiteral("id");
    QVariantList m_values;
};

}
