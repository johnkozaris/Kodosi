#pragma once
#include "accessibility/AccessibilityScope.hpp"
#include "app/ApplicationLifecycleModel.hpp"
#include "presentation/AccountModel.hpp"
#include "presentation/AppearanceModel.hpp"
#include "presentation/ConversationHistoryModel.hpp"
#include "presentation/DesktopSettings.hpp"
#include "presentation/DesktopStateModel.hpp"
#include "presentation/DevicesModel.hpp"
#include "presentation/MissionsModel.hpp"
#include "presentation/PeopleModel.hpp"
#include "presentation/ProviderFilesModel.hpp"
#include "presentation/SessionCatalogModel.hpp"
#include "presentation/SessionActionsModel.hpp"
#include "presentation/TerminalTilingLayoutModel.hpp"
#include "presentation/Workspace.hpp"
#include "platform/DesktopFileIntegration.hpp"
#include "terminal/TerminalSurfaceController.hpp"
#include "terminal/TerminalView.hpp"
#include <QJSEngine>
#include <QQmlEngine>
#include <QtQml/qqmlregistration.h>
namespace kodosi::qml {
template <typename T> T* singleton(T* instance, QQmlEngine* engine)
{
    Q_ASSERT(instance && engine && instance->thread() == engine->thread());
    QQmlEngine::setObjectOwnership(instance, QQmlEngine::CppOwnership);
    return instance;
}
struct AppStateForeign {
    Q_GADGET
    QML_FOREIGN(kodosi::Workspace)
    QML_NAMED_ELEMENT(AppState)
    QML_SINGLETON
public:
    inline static kodosi::Workspace* instance = nullptr;
    static kodosi::Workspace* create(QQmlEngine* engine, QJSEngine*) { return singleton(instance, engine); }
};

struct AccountModelForeign {
    Q_GADGET
    QML_FOREIGN(kodosi::AccountModel)
    QML_NAMED_ELEMENT(Account)
    QML_SINGLETON
public:
    inline static kodosi::AccountModel* instance = nullptr;
    static kodosi::AccountModel* create(QQmlEngine* engine, QJSEngine*) { return singleton(instance, engine); }
};

struct PeopleModelForeign {
    Q_GADGET
    QML_FOREIGN(kodosi::PeopleModel)
    QML_NAMED_ELEMENT(People)
    QML_SINGLETON
public:
    inline static kodosi::PeopleModel* instance = nullptr;
    static kodosi::PeopleModel* create(QQmlEngine* engine, QJSEngine*) { return singleton(instance, engine); }
};

struct DevicesModelForeign {
    Q_GADGET
    QML_FOREIGN(kodosi::DevicesModel)
    QML_NAMED_ELEMENT(Devices)
    QML_SINGLETON
public:
    inline static kodosi::DevicesModel* instance = nullptr;
    static kodosi::DevicesModel* create(QQmlEngine* engine, QJSEngine*) { return singleton(instance, engine); }
};

struct MissionsModelForeign {
    Q_GADGET
    QML_FOREIGN(kodosi::MissionsModel)
    QML_NAMED_ELEMENT(Missions)
    QML_SINGLETON
public:
    inline static kodosi::MissionsModel* instance = nullptr;
    static kodosi::MissionsModel* create(QQmlEngine* engine, QJSEngine*) { return singleton(instance, engine); }
};

struct SessionActionsModelForeign {
    Q_GADGET
    QML_FOREIGN(kodosi::SessionActionsModel)
    QML_NAMED_ELEMENT(SessionActions)
    QML_SINGLETON
public:
    inline static kodosi::SessionActionsModel* instance = nullptr;
    static kodosi::SessionActionsModel* create(QQmlEngine* engine, QJSEngine*) { return singleton(instance, engine); }
};

struct ConversationHistoryModelForeign {
    Q_GADGET
    QML_FOREIGN(kodosi::ConversationHistoryModel)
    QML_NAMED_ELEMENT(ConversationHistory)
    QML_SINGLETON
public:
    inline static kodosi::ConversationHistoryModel* instance = nullptr;
    static kodosi::ConversationHistoryModel* create(QQmlEngine* engine, QJSEngine*) { return singleton(instance, engine); }
};

struct ProviderFilesModelForeign {
    Q_GADGET
    QML_FOREIGN(kodosi::ProviderFilesModel)
    QML_NAMED_ELEMENT(ProviderFiles)
    QML_SINGLETON
public:
    inline static kodosi::ProviderFilesModel* instance = nullptr;
    static kodosi::ProviderFilesModel* create(QQmlEngine* engine, QJSEngine*) { return singleton(instance, engine); }
};

struct AppearanceModelForeign {
    Q_GADGET
    QML_FOREIGN(kodosi::AppearanceModel)
    QML_NAMED_ELEMENT(Appearance)
    QML_SINGLETON
public:
    inline static kodosi::AppearanceModel* instance = nullptr;
    static kodosi::AppearanceModel* create(QQmlEngine* engine, QJSEngine*) { return singleton(instance, engine); }
};

struct DesktopSettingsForeign {
    Q_GADGET
    QML_FOREIGN(kodosi::DesktopSettings)
    QML_NAMED_ELEMENT(DesktopSettings)
    QML_SINGLETON
public:
    inline static kodosi::DesktopSettings* instance = nullptr;
    static kodosi::DesktopSettings* create(QQmlEngine* engine, QJSEngine*) { return singleton(instance, engine); }
};

struct DesktopStateModelForeign {
    Q_GADGET
    QML_FOREIGN(kodosi::DesktopStateModel)
    QML_NAMED_ELEMENT(DesktopState)
    QML_SINGLETON
public:
    inline static kodosi::DesktopStateModel* instance = nullptr;
    static kodosi::DesktopStateModel* create(QQmlEngine* engine, QJSEngine*) { return singleton(instance, engine); }
};

struct SessionCatalogModelForeign {
    Q_GADGET
    QML_FOREIGN(kodosi::SessionCatalogModel)
    QML_NAMED_ELEMENT(Sessions)
    QML_SINGLETON
public:
    inline static kodosi::SessionCatalogModel* instance = nullptr;
    static kodosi::SessionCatalogModel* create(QQmlEngine* engine, QJSEngine*) { return singleton(instance, engine); }
};

struct TerminalTilingLayoutModelForeign {
    Q_GADGET
    QML_FOREIGN(kodosi::TerminalTilingLayoutModel)
    QML_NAMED_ELEMENT(TerminalTiling)
    QML_SINGLETON
public:
    inline static kodosi::TerminalTilingLayoutModel* instance = nullptr;
    static kodosi::TerminalTilingLayoutModel* create(QQmlEngine* engine, QJSEngine*) { return singleton(instance, engine); }
};

struct DesktopFileIntegrationForeign {
    Q_GADGET
    QML_FOREIGN(kodosi::DesktopFileIntegration)
    QML_NAMED_ELEMENT(DesktopFiles)
    QML_SINGLETON
public:
    inline static kodosi::DesktopFileIntegration* instance = nullptr;
    static kodosi::DesktopFileIntegration* create(QQmlEngine* engine, QJSEngine*) { return singleton(instance, engine); }
};

struct ApplicationLifecycleModelForeign {
    Q_GADGET
    QML_FOREIGN(kodosi::ApplicationLifecycleModel)
    QML_NAMED_ELEMENT(ApplicationLifecycle)
    QML_SINGLETON
public:
    inline static kodosi::ApplicationLifecycleModel* instance = nullptr;
    static kodosi::ApplicationLifecycleModel* create(QQmlEngine* engine, QJSEngine*) { return singleton(instance, engine); }
};

struct TerminalSurfaceControllerForeign {
    Q_GADGET
    QML_FOREIGN(kodosi::TerminalSurfaceController)
    QML_NAMED_ELEMENT(TerminalSurfaces)
    QML_SINGLETON
public:
    inline static kodosi::TerminalSurfaceController* instance = nullptr;
    static kodosi::TerminalSurfaceController* create(QQmlEngine* engine, QJSEngine*) { return singleton(instance, engine); }
};

struct TerminalViewForeign {
    Q_GADGET
    QML_FOREIGN(kodosi::TerminalView)
    QML_NAMED_ELEMENT(TerminalView)
};

struct AccessibilityScopeForeign {
    Q_GADGET
    QML_FOREIGN(kodosi::AccessibilityScope)
    QML_NAMED_ELEMENT(AccessibilityScope)
};
void configureModelInstances(Workspace&, ConversationHistoryModel&, ProviderFilesModel&,
    AppearanceModel&, DesktopSettings&,
    DesktopStateModel&, SessionCatalogModel&, TerminalTilingLayoutModel&, DesktopFileIntegration&,
    ApplicationLifecycleModel&, TerminalSurfaceController&);
}
