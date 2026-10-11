#pragma once

#include <QSettings>
#include <QString>
#include <expected>
#include <memory>

namespace kodosi {

struct RuntimeStorageBootstrap {
    QString runtimeRoot;
    QString isolatedRoot;
    QString preferencesPath;
    QString logDirectory;

    [[nodiscard]] static std::expected<RuntimeStorageBootstrap, QString> resolve();
    [[nodiscard]] std::expected<void, QString> prepare() const;
    [[nodiscard]] std::unique_ptr<QSettings> settings() const;
};

}
