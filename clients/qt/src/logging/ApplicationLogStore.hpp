#pragma once

#include <QElapsedTimer>
#include <QLoggingCategory>
#include <QString>
#include <QStringView>

#include <mutex>

Q_DECLARE_LOGGING_CATEGORY(kodosiPerformance)

namespace kodosi {

class ApplicationLogStore final {
public:
    struct Options {
        QString directory;
        qint64 rotationBytes = 2 * 1024 * 1024;
        int maximumArchives = 5;
        int maximumCompletedFamilies = 8;
        bool installQtMessageHandler = true;
    };

    explicit ApplicationLogStore(Options options);
    ~ApplicationLogStore();

    ApplicationLogStore(const ApplicationLogStore&) = delete;
    ApplicationLogStore& operator=(const ApplicationLogStore&) = delete;

    [[nodiscard]] bool healthy() const;
    [[nodiscard]] QString path() const;
    [[nodiscard]] QString directory() const;
    [[nodiscard]] qint64 sizeBytes() const;
    [[nodiscard]] int rotationCount() const;
    [[nodiscard]] QString lastError() const;

    void record(
        QtMsgType type,
        QStringView category,
        QStringView message);

    [[nodiscard]] static QString redact(QString message);

private:
    friend void applicationQtMessageHandler(
        QtMsgType,
        const QMessageLogContext&,
        const QString&);

    static constexpr qsizetype maximumPersistedMessageCharacters = 32 * 1024;
    static constexpr qsizetype maximumCategoryCharacters = 256;

    Options m_options;
    QString m_path;
    mutable std::mutex m_mutex;
    QtMessageHandler m_previousHandler = nullptr;
    QString m_lastError;
    qint64 m_sizeBytes = 0;
    int m_rotationCount = 0;
    int m_fileDescriptor = -1;
    bool m_healthy = false;
    bool m_handlerInstalled = false;

    void initialize();
    [[nodiscard]] bool openCurrentFile();
    [[nodiscard]] bool rotate();
    [[nodiscard]] bool writeAll(const QByteArray& bytes);
    [[nodiscard]] bool sanitizeCurrentFamilyLocked();
    [[nodiscard]] bool pruneCompletedFamiliesLocked();
    void failLocked(QString error);
    void refreshArchiveCountLocked();
};

enum class PerformanceCategory {
    Terminal,
};

class ScopedPerformanceSpan final {
public:
    ScopedPerformanceSpan(
        PerformanceCategory category,
        QStringView operation,
        qint64 thresholdMilliseconds);
    ~ScopedPerformanceSpan();

    ScopedPerformanceSpan(const ScopedPerformanceSpan&) = delete;
    ScopedPerformanceSpan& operator=(const ScopedPerformanceSpan&) = delete;

    void setOutcome(QStringView outcome);

private:
    PerformanceCategory m_category;
    QString m_operation;
    QString m_outcome = QStringLiteral("ok");
    qint64 m_thresholdMilliseconds = 0;
    QElapsedTimer m_timer;
};

}
