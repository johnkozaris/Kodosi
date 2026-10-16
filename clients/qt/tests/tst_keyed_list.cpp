#include "presentation/KeyedListModel.hpp"
#include <QAbstractItemModelTester>
#include <QSignalSpy>
#include <QtTest/QTest>

namespace {
QVariantMap entry(const QString& id, int version)
{
    return { { QStringLiteral("id"), id }, { QStringLiteral("version"), version } };
}

QStringList ids(const kodosi::KeyedListModel& model)
{
    QStringList result;
    for (int row = 0; row < model.rowCount(); ++row)
        result.append(model.data(model.index(row), Qt::UserRole).toMap().value(QStringLiteral("id")).toString());
    return result;
}
}

class KeyedListTest final : public QObject {
    Q_OBJECT
private slots:
    void rowsKeepTheirPlaceWhileValuesChange()
    {
        kodosi::KeyedListModel model;
        QAbstractItemModelTester tester(&model, QAbstractItemModelTester::FailureReportingMode::QtTest);
        QSignalSpy resets(&model, &QAbstractItemModel::modelReset);
        QSignalSpy inserted(&model, &QAbstractItemModel::rowsInserted);
        QSignalSpy removed(&model, &QAbstractItemModel::rowsRemoved);
        QSignalSpy moved(&model, &QAbstractItemModel::rowsMoved);
        QSignalSpy changed(&model, &QAbstractItemModel::dataChanged);

        model.setValues({ entry(QStringLiteral("a"), 1), entry(QStringLiteral("b"), 1), entry(QStringLiteral("c"), 1) });
        QCOMPARE(ids(model), (QStringList { QStringLiteral("a"), QStringLiteral("b"), QStringLiteral("c") }));
        QCOMPARE(inserted.count(), 3);

        model.setValues({ entry(QStringLiteral("a"), 1), entry(QStringLiteral("b"), 2), entry(QStringLiteral("c"), 1) });
        QCOMPARE(changed.count(), 1);
        QCOMPARE(changed.first().first().toModelIndex().row(), 1);
        QCOMPARE(model.data(model.index(1), Qt::UserRole).toMap().value(QStringLiteral("version")).toInt(), 2);

        model.setValues({ entry(QStringLiteral("c"), 1), entry(QStringLiteral("d"), 1), entry(QStringLiteral("a"), 1) });
        QCOMPARE(ids(model), (QStringList { QStringLiteral("c"), QStringLiteral("d"), QStringLiteral("a") }));
        QCOMPARE(removed.count(), 1);
        QCOMPARE(moved.count(), 1);
        QCOMPARE(inserted.count(), 4);

        const auto before = changed.count() + inserted.count() + removed.count() + moved.count();
        model.setValues(model.values());
        QCOMPARE(changed.count() + inserted.count() + removed.count() + moved.count(), before);
        QCOMPARE(resets.count(), 0);
        QCOMPARE(model.roleNames().value(Qt::UserRole), QByteArrayLiteral("modelData"));
    }

    void aRepeatedKeyKeepsOneRowEach()
    {
        kodosi::KeyedListModel model;
        QAbstractItemModelTester tester(&model, QAbstractItemModelTester::FailureReportingMode::QtTest);
        model.setValues({ entry(QStringLiteral("a"), 1), entry(QStringLiteral("a"), 2) });
        QCOMPARE(model.rowCount(), 2);
        model.setValues({ entry(QStringLiteral("a"), 3) });
        QCOMPARE(model.rowCount(), 1);
        QCOMPARE(model.data(model.index(0), Qt::UserRole).toMap().value(QStringLiteral("version")).toInt(), 3);
        model.setValues({});
        QCOMPARE(model.rowCount(), 0);
    }
};

QTEST_GUILESS_MAIN(KeyedListTest)
#include "tst_keyed_list.moc"
