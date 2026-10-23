#pragma once

#include <QObject>
#include <QString>
#include <QTimer>

#include <expected>
#include <memory>
#include <optional>
#include <string>
#include <string_view>

namespace winrt::Windows::ApplicationModel {
struct PackageVersion;
}

namespace winrt::Windows::Data::Xml::Dom {
struct XmlDocument;
}

namespace kodosi {

// While Kodosi runs, Windows only stages a newer package; it installs the package when
// Kodosi starts again.
class PackageUpdate final : public QObject {
    Q_OBJECT

public:
    explicit PackageUpdate(QObject* parent = nullptr);
    ~PackageUpdate() override;
    PackageUpdate(const PackageUpdate&) = delete;
    PackageUpdate& operator=(const PackageUpdate&) = delete;

    [[nodiscard]] static bool openAgainAfterExit();

signals:
    void staged();

private:
    struct Relay;
    std::shared_ptr<Relay> m_relay;
    QTimer m_timer;
    bool m_checking = false;

    void check();
    void finish(const std::expected<bool, QString>& result);
};

[[nodiscard]] std::optional<std::wstring> newerBundle(const winrt::Windows::Data::Xml::Dom::XmlDocument& installer,
    std::wstring_view name, std::wstring_view publisher, const winrt::Windows::ApplicationModel::PackageVersion& version);

}
