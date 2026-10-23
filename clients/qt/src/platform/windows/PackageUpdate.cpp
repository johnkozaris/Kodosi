#include "platform/windows/PackageUpdate.hpp"

#include <QDebug>
#include <QMetaObject>
#include <QVersionNumber>

#include <array>
#include <chrono>
#include <format>
#include <mutex>
#include <thread>
#include <utility>

#include <windows.h>

#include <appmodel.h>

#include <winrt/Windows.ApplicationModel.h>
#include <winrt/Windows.Data.Xml.Dom.h>
#include <winrt/Windows.Foundation.Collections.h>
#include <winrt/Windows.Foundation.h>
#include <winrt/Windows.Management.Deployment.h>
#include <winrt/Windows.Web.Http.h>

namespace kodosi {
namespace {

namespace model = winrt::Windows::ApplicationModel;
namespace deployment = winrt::Windows::Management::Deployment;
using winrt::Windows::Data::Xml::Dom::XmlDocument;
using winrt::Windows::Data::Xml::Dom::XmlElement;

constexpr std::chrono::minutes firstCheck(1);
constexpr std::chrono::hours checkInterval(8);

QString text(const std::wstring_view value)
{
    return QString::fromWCharArray(value.data(), static_cast<qsizetype>(value.size()));
}

std::expected<bool, QString> stage()
{
    try {
        const auto id = model::Package::Current().Id();
        deployment::PackageManager manager;
        // CheckUpdateAvailabilityAsync refuses the Package that Package::Current returns.
        const auto package = manager.FindPackageForUser({}, id.FullName());
        const auto installer = package.GetAppInstallerInfo();
        if (!installer)
            return false;
        const auto availability = package.CheckUpdateAvailabilityAsync().get().Availability();
        if (availability != model::PackageUpdateAvailability::Available
            && availability != model::PackageUpdateAvailability::Required)
            return false;
        XmlDocument file;
        file.LoadXml(winrt::Windows::Web::Http::HttpClient().GetStringAsync(installer.Uri()).get());
        const auto bundle = newerBundle(file, id.Name(), id.Publisher(), id.Version());
        if (!bundle)
            return false;
        deployment::AddPackageOptions options;
        options.DeferRegistrationWhenPackagesAreInUse(true);
        manager.AddPackageByUriAsync(winrt::Windows::Foundation::Uri(*bundle), options).get();
        return true;
    } catch (const winrt::hresult_error& error) {
        return std::unexpected(text(error.message()));
    }
}

}

std::optional<std::wstring> newerBundle(const XmlDocument& installer, const std::wstring_view name,
    const std::wstring_view publisher, const model::PackageVersion& version)
{
    const auto bundles = installer.GetElementsByTagName(L"MainBundle");
    if (bundles.Size() != 1)
        return std::nullopt;
    const auto bundle = bundles.Item(0).as<XmlElement>();
    const auto offered = QVersionNumber::fromString(text(bundle.GetAttribute(L"Version")));
    if (std::wstring_view(bundle.GetAttribute(L"Name")) != name
        || std::wstring_view(bundle.GetAttribute(L"Publisher")) != publisher
        || offered <= QVersionNumber({ version.Major, version.Minor, version.Build, version.Revision }))
        return std::nullopt;
    const auto uri = bundle.GetAttribute(L"Uri");
    if (uri.empty())
        return std::nullopt;
    return std::wstring(uri);
}

// Results arrive on a worker thread and may outlive the update.
struct PackageUpdate::Relay {
    std::mutex mutex;
    PackageUpdate* update = nullptr;
};

PackageUpdate::PackageUpdate(QObject* parent)
    : QObject(parent)
    , m_relay(std::make_shared<Relay>())
{
    m_relay->update = this;
    UINT32 length = 0;
    if (GetCurrentPackageFullName(&length, nullptr) == APPMODEL_ERROR_NO_PACKAGE)
        return;
    connect(&m_timer, &QTimer::timeout, this, &PackageUpdate::check);
    m_timer.start(checkInterval);
    QTimer::singleShot(firstCheck, this, &PackageUpdate::check);
}

PackageUpdate::~PackageUpdate()
{
    const std::lock_guard lock(m_relay->mutex);
    m_relay->update = nullptr;
}

void PackageUpdate::check()
{
    if (std::exchange(m_checking, true))
        return;
    std::thread([relay = m_relay] {
        winrt::init_apartment();
        const auto result = stage();
        winrt::uninit_apartment();
        const std::lock_guard lock(relay->mutex);
        if (relay->update)
            QMetaObject::invokeMethod(
                relay->update, [update = relay->update, result] { update->finish(result); }, Qt::QueuedConnection);
    }).detach();
}

void PackageUpdate::finish(const std::expected<bool, QString>& result)
{
    m_checking = false;
    if (!result) {
        qWarning().noquote() << "The Windows update could not be staged:" << result.error();
    } else if (*result) {
        m_timer.stop();
        emit staged();
    }
}

bool PackageUpdate::openAgainAfterExit()
{
    UINT32 length = 0;
    if (GetCurrentApplicationUserModelId(&length, nullptr) != ERROR_INSUFFICIENT_BUFFER)
        return false;
    std::wstring app(length, L'\0');
    if (GetCurrentApplicationUserModelId(&length, app.data()) != ERROR_SUCCESS)
        return false;
    app.resize(length - 1);
    std::array<wchar_t, MAX_PATH> folder {};
    const auto size = GetSystemDirectoryW(folder.data(), static_cast<UINT>(folder.size()));
    if (size == 0 || size >= folder.size())
        return false;
    const std::wstring system(folder.data(), size);
    const auto program = system + LR"(\WindowsPowerShell\v1.0\powershell.exe)";
    // Windows installs the staged package when Kodosi starts while no program of the package
    // runs, so a program from outside the package waits for this one to end.
    auto line = std::format(LR"("{}" -NoProfile -NonInteractive -Command "Wait-Process -Id {} -ErrorAction Ignore; )"
                            LR"(Start-Process 'shell:AppsFolder\{}'")",
        program, GetCurrentProcessId(), app);
    STARTUPINFOW startup {};
    startup.cb = sizeof(startup);
    PROCESS_INFORMATION process {};
    if (!CreateProcessW(program.c_str(), line.data(), nullptr, nullptr, FALSE, CREATE_NO_WINDOW, nullptr,
            system.c_str(), &startup, &process))
        return false;
    AllowSetForegroundWindow(process.dwProcessId);
    CloseHandle(process.hThread);
    CloseHandle(process.hProcess);
    return true;
}

}
