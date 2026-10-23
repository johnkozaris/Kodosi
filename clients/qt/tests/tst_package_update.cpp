#include "platform/windows/PackageUpdate.hpp"

#include <QString>
#include <QTest>

#include <format>
#include <string>
#include <string_view>

#include <winrt/Windows.ApplicationModel.h>
#include <winrt/Windows.Data.Xml.Dom.h>

namespace {

QString bundle(const std::wstring_view name, const std::wstring_view publisher, const std::wstring_view version)
{
    winrt::Windows::Data::Xml::Dom::XmlDocument installer;
    installer.LoadXml(std::format(L"<AppInstaller xmlns='http://schemas.microsoft.com/appx/appinstaller/2021' "
                                  L"Version='{2}' Uri='https://updates.kodosi.com/windows/Kodosi.appinstaller'>"
                                  L"<MainBundle Name='{0}' Publisher='{1}' Version='{2}' Uri='{3}' />"
                                  L"<UpdateSettings><OnLaunch HoursBetweenUpdateChecks='8' /></UpdateSettings>"
                                  L"</AppInstaller>",
        name, publisher, version,
        L"https://github.com/johnkozaris/Kodosi/releases/download/v0.3.0/Kodosi-0.3.0.msixbundle"));
    return QString::fromStdWString(
        kodosi::newerBundle(installer, L"com.kodosi.Kodosi", L"CN=Kodosi", { 0, 2, 0, 0 }).value_or(std::wstring()));
}

}

class PackageUpdateTest final : public QObject {
    Q_OBJECT

private slots:
    void stagesOnlyANewerBundleOfThisPackage();
};

void PackageUpdateTest::stagesOnlyANewerBundleOfThisPackage()
{
    QCOMPARE(bundle(L"com.kodosi.Kodosi", L"CN=Kodosi", L"0.3.0.0"),
        QStringLiteral("https://github.com/johnkozaris/Kodosi/releases/download/v0.3.0/Kodosi-0.3.0.msixbundle"));
    QCOMPARE(bundle(L"com.kodosi.Kodosi", L"CN=Kodosi", L"0.2.0.0"), QString());
    QCOMPARE(bundle(L"com.example.Kodosi", L"CN=Kodosi", L"0.3.0.0"), QString());
    QCOMPARE(bundle(L"com.kodosi.Kodosi", L"CN=Kodosi, O=Example", L"0.3.0.0"), QString());
}

QTEST_GUILESS_MAIN(PackageUpdateTest)

#include "tst_package_update.moc"
