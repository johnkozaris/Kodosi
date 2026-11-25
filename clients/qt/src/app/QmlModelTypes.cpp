#include "app/QmlModelTypes.hpp"
namespace kodosi::qml {
void provideModels(QQmlEngine& engine, Workspace& workspace, ConversationHistoryModel& history,
    ProviderFilesModel& providerFiles, AgentLauncherModel& launcher, AppearanceModel& appearance,
    DesktopSettings& settings,
    DesktopStateModel& desktop, SessionCatalogModel& sessions, TerminalTilingLayoutModel& tiling,
    DesktopFileIntegration& files, ApplicationLifecycleModel& lifecycle,
    TerminalSurfaceController& surfaces, DesktopTray& tray, CanvasModel& canvas, CanvasDocuments& documents,
    CanvasWeb& canvasWeb, ShortcutsModel& shortcuts, TerminalChats& chats, ChatProse& prose)
{
    provide<AppStateForeign>(engine, workspace);
    provide<AccountModelForeign>(engine, workspace.account());
    provide<PeopleModelForeign>(engine, workspace.people());
    provide<DevicesModelForeign>(engine, workspace.devices());
    provide<ProjectsModelForeign>(engine, workspace.projects());
    provide<DosiModelForeign>(engine, workspace.dosi());
    provide<SessionActionsModelForeign>(engine, workspace.sessionActions());
    provide<SharingModelForeign>(engine, workspace.sharing());
    provide<ConversationHistoryModelForeign>(engine, history);
    provide<ProviderFilesModelForeign>(engine, providerFiles);
    provide<AgentLauncherModelForeign>(engine, launcher);
    provide<AppearanceModelForeign>(engine, appearance);
    provide<DesktopSettingsForeign>(engine, settings);
    provide<DesktopStateModelForeign>(engine, desktop);
    provide<SessionCatalogModelForeign>(engine, sessions);
    provide<TerminalTilingLayoutModelForeign>(engine, tiling);
    provide<DesktopFileIntegrationForeign>(engine, files);
    provide<ApplicationLifecycleModelForeign>(engine, lifecycle);
    provide<TerminalSurfaceControllerForeign>(engine, surfaces);
    provide<DesktopTrayForeign>(engine, tray);
    provide<CanvasModelForeign>(engine, canvas);
    provide<CanvasDocumentsForeign>(engine, documents);
    provide<CanvasWebForeign>(engine, canvasWeb);
    provide<ShortcutsModelForeign>(engine, shortcuts);
    provide<TerminalChatsForeign>(engine, chats);
    provide<ChatProseForeign>(engine, prose);
}
}
