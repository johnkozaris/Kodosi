#pragma once
#include "accessibility/AccessibilityScope.hpp"
#include "app/ApplicationLifecycleModel.hpp"
#include "canvas/BrowserVideo.hpp"
#include "canvas/ScreenVideo.hpp"
#include "canvas/CanvasWeb.hpp"
#include "presentation/AccountModel.hpp"
#include "presentation/AppearanceModel.hpp"
#include "presentation/CanvasDocuments.hpp"
#include "presentation/ChatProse.hpp"
#include "presentation/TerminalChat.hpp"
#include "presentation/CanvasModel.hpp"
#include "presentation/ConversationHistoryModel.hpp"
#include "presentation/DesktopSettings.hpp"
#include "presentation/DesktopStateModel.hpp"
#include "presentation/AgentLauncherModel.hpp"
#include "presentation/DevicesModel.hpp"
#include "presentation/DosiModel.hpp"
#include "presentation/KeyedListModel.hpp"
#include "presentation/ProjectsModel.hpp"
#include "presentation/PeopleModel.hpp"
#include "presentation/ProviderFilesModel.hpp"
#include "presentation/SessionCatalogModel.hpp"
#include "presentation/SessionActionsModel.hpp"
#include "presentation/SharingModel.hpp"
#include "presentation/ShortcutsModel.hpp"
#include "presentation/TerminalTilingLayoutModel.hpp"
#include "presentation/Workspace.hpp"
#include "platform/DesktopFileIntegration.hpp"
#include "platform/DesktopTray.hpp"
#include "terminal/TerminalSurfaceController.hpp"
#include "terminal/TerminalView.hpp"
#include <QQmlEngine>
#include <QtQml/qqmlregistration.h>
namespace kodosi::qml {
struct AppStateForeign {
    Q_GADGET
    QML_FOREIGN(kodosi::Workspace)
    QML_NAMED_ELEMENT(AppState)
    QML_SINGLETON
    QML_UNCREATABLE("Kodosi provides it.")
};

struct AccountModelForeign {
    Q_GADGET
    QML_FOREIGN(kodosi::AccountModel)
    QML_NAMED_ELEMENT(Account)
    QML_SINGLETON
    QML_UNCREATABLE("Kodosi provides it.")
};

struct PeopleModelForeign {
    Q_GADGET
    QML_FOREIGN(kodosi::PeopleModel)
    QML_NAMED_ELEMENT(People)
    QML_SINGLETON
    QML_UNCREATABLE("Kodosi provides it.")
};

struct DevicesModelForeign {
    Q_GADGET
    QML_FOREIGN(kodosi::DevicesModel)
    QML_NAMED_ELEMENT(Devices)
    QML_SINGLETON
    QML_UNCREATABLE("Kodosi provides it.")
};

struct ProjectsModelForeign {
    Q_GADGET
    QML_FOREIGN(kodosi::ProjectsModel)
    QML_NAMED_ELEMENT(Projects)
    QML_SINGLETON
    QML_UNCREATABLE("Kodosi provides it.")
};

struct DosiModelForeign {
    Q_GADGET
    QML_FOREIGN(kodosi::DosiModel)
    QML_NAMED_ELEMENT(Dosi)
    QML_SINGLETON
    QML_UNCREATABLE("Kodosi provides it.")
};

struct SessionActionsModelForeign {
    Q_GADGET
    QML_FOREIGN(kodosi::SessionActionsModel)
    QML_NAMED_ELEMENT(SessionActions)
    QML_SINGLETON
    QML_UNCREATABLE("Kodosi provides it.")
};

struct SharingModelForeign {
    Q_GADGET
    QML_FOREIGN(kodosi::SharingModel)
    QML_NAMED_ELEMENT(Sharing)
    QML_SINGLETON
    QML_UNCREATABLE("Kodosi provides it.")
};

struct ConversationHistoryModelForeign {
    Q_GADGET
    QML_FOREIGN(kodosi::ConversationHistoryModel)
    QML_NAMED_ELEMENT(ConversationHistory)
    QML_SINGLETON
    QML_UNCREATABLE("Kodosi provides it.")
};

struct ProviderFilesModelForeign {
    Q_GADGET
    QML_FOREIGN(kodosi::ProviderFilesModel)
    QML_NAMED_ELEMENT(ProviderFiles)
    QML_SINGLETON
    QML_UNCREATABLE("Kodosi provides it.")
};

struct AgentLauncherModelForeign {
    Q_GADGET
    QML_FOREIGN(kodosi::AgentLauncherModel)
    QML_NAMED_ELEMENT(Launcher)
    QML_SINGLETON
    QML_UNCREATABLE("Kodosi provides it.")
};

struct AppearanceModelForeign {
    Q_GADGET
    QML_FOREIGN(kodosi::AppearanceModel)
    QML_NAMED_ELEMENT(Appearance)
    QML_SINGLETON
    QML_UNCREATABLE("Kodosi provides it.")
};

struct DesktopSettingsForeign {
    Q_GADGET
    QML_FOREIGN(kodosi::DesktopSettings)
    QML_NAMED_ELEMENT(DesktopSettings)
    QML_SINGLETON
    QML_UNCREATABLE("Kodosi provides it.")
};

struct DesktopStateModelForeign {
    Q_GADGET
    QML_FOREIGN(kodosi::DesktopStateModel)
    QML_NAMED_ELEMENT(DesktopState)
    QML_SINGLETON
    QML_UNCREATABLE("Kodosi provides it.")
};

struct SessionCatalogModelForeign {
    Q_GADGET
    QML_FOREIGN(kodosi::SessionCatalogModel)
    QML_NAMED_ELEMENT(Sessions)
    QML_SINGLETON
    QML_UNCREATABLE("Kodosi provides it.")
};

struct ShortcutsModelForeign {
    Q_GADGET
    QML_FOREIGN(kodosi::ShortcutsModel)
    QML_NAMED_ELEMENT(Shortcuts)
    QML_SINGLETON
    QML_UNCREATABLE("Kodosi provides it.")
};

struct TerminalTilingLayoutModelForeign {
    Q_GADGET
    QML_FOREIGN(kodosi::TerminalTilingLayoutModel)
    QML_NAMED_ELEMENT(TerminalTiling)
    QML_SINGLETON
    QML_UNCREATABLE("Kodosi provides it.")
};

struct DesktopFileIntegrationForeign {
    Q_GADGET
    QML_FOREIGN(kodosi::DesktopFileIntegration)
    QML_NAMED_ELEMENT(DesktopFiles)
    QML_SINGLETON
    QML_UNCREATABLE("Kodosi provides it.")
};

struct DesktopTrayForeign {
    Q_GADGET
    QML_FOREIGN(kodosi::DesktopTray)
    QML_NAMED_ELEMENT(Tray)
    QML_SINGLETON
    QML_UNCREATABLE("Kodosi provides it.")
};

struct ApplicationLifecycleModelForeign {
    Q_GADGET
    QML_FOREIGN(kodosi::ApplicationLifecycleModel)
    QML_NAMED_ELEMENT(ApplicationLifecycle)
    QML_SINGLETON
    QML_UNCREATABLE("Kodosi provides it.")
};

struct TerminalSurfaceControllerForeign {
    Q_GADGET
    QML_FOREIGN(kodosi::TerminalSurfaceController)
    QML_NAMED_ELEMENT(TerminalSurfaces)
    QML_SINGLETON
    QML_UNCREATABLE("Kodosi provides it.")
};

struct CanvasModelForeign {
    Q_GADGET
    QML_FOREIGN(kodosi::CanvasModel)
    QML_NAMED_ELEMENT(Canvas)
    QML_SINGLETON
    QML_UNCREATABLE("Kodosi provides it.")
};

struct CanvasDocumentsForeign {
    Q_GADGET
    QML_FOREIGN(kodosi::CanvasDocuments)
    QML_NAMED_ELEMENT(CanvasDocuments)
    QML_SINGLETON
    QML_UNCREATABLE("Kodosi provides it.")
};

struct TerminalChatsForeign {
    Q_GADGET
    QML_FOREIGN(kodosi::TerminalChats)
    QML_NAMED_ELEMENT(Chats)
    QML_SINGLETON
    QML_UNCREATABLE("Kodosi provides it.")
};

struct TerminalChatForeign {
    Q_GADGET
    QML_FOREIGN(kodosi::TerminalChat)
    QML_NAMED_ELEMENT(TerminalChat)
    QML_UNCREATABLE("A terminal's chat comes from Chats.")
};

struct ChatProseForeign {
    Q_GADGET
    QML_FOREIGN(kodosi::ChatProse)
    QML_NAMED_ELEMENT(Markdown)
    QML_SINGLETON
    QML_UNCREATABLE("Kodosi provides it.")
};

struct CanvasWebForeign {
    Q_GADGET
    QML_FOREIGN(kodosi::CanvasWeb)
    QML_NAMED_ELEMENT(CanvasWeb)
    QML_SINGLETON
    QML_UNCREATABLE("Kodosi provides it.")
};

struct StreamVideoForeign {
    Q_GADGET
    QML_FOREIGN(kodosi::StreamVideo)
    QML_ANONYMOUS
};

struct BrowserVideoForeign {
    Q_GADGET
    QML_FOREIGN(kodosi::BrowserVideo)
    QML_NAMED_ELEMENT(BrowserVideo)
};

struct ScreenVideoForeign {
    Q_GADGET
    QML_FOREIGN(kodosi::ScreenVideo)
    QML_NAMED_ELEMENT(ScreenVideo)
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

struct KeyedListModelForeign {
    Q_GADGET
    QML_FOREIGN(kodosi::KeyedListModel)
    QML_NAMED_ELEMENT(KeyedList)
};
template <typename Foreign> void provide(QQmlEngine& engine, QObject& instance)
{
    const auto& type = Foreign::staticMetaObject;
    engine.setExternalSingletonInstance(
        "Kodosi.Models", type.classInfo(type.indexOfClassInfo("QML.Element")).value(), &instance);
}

void provideModels(QQmlEngine&, Workspace&, ConversationHistoryModel&, ProviderFilesModel&,
    AgentLauncherModel&, AppearanceModel&, DesktopSettings&,
    DesktopStateModel&, SessionCatalogModel&, TerminalTilingLayoutModel&, DesktopFileIntegration&,
    ApplicationLifecycleModel&, TerminalSurfaceController&, DesktopTray&, CanvasModel&, CanvasDocuments&,
    CanvasWeb&, ShortcutsModel&, TerminalChats&, ChatProse&);
}
