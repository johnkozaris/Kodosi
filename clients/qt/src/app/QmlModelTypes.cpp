#include "app/QmlModelTypes.hpp"
namespace kodosi::qml {
void configureModelInstances(Workspace& workspace, ConversationHistoryModel& history,
    ProviderFilesModel& providerFiles, AgentLauncherModel& launcher, AppearanceModel& appearance,
    DesktopSettings& settings,
    DesktopStateModel& desktop, SessionCatalogModel& sessions, TerminalTilingLayoutModel& tiling,
    DesktopFileIntegration& files, ApplicationLifecycleModel& lifecycle,
    TerminalSurfaceController& surfaces, DesktopTray& tray)
{
    AppStateForeign::instance = &workspace;
    AccountModelForeign::instance = &workspace.account();
    PeopleModelForeign::instance = &workspace.people();
    DevicesModelForeign::instance = &workspace.devices();
    MissionsModelForeign::instance = &workspace.missions();
    DosiModelForeign::instance = &workspace.dosi();
    SessionActionsModelForeign::instance = &workspace.sessionActions();
    ConversationHistoryModelForeign::instance = &history;
    ProviderFilesModelForeign::instance = &providerFiles;
    AgentLauncherModelForeign::instance = &launcher;
    AppearanceModelForeign::instance = &appearance;
    DesktopSettingsForeign::instance = &settings;
    DesktopStateModelForeign::instance = &desktop;
    SessionCatalogModelForeign::instance = &sessions;
    TerminalTilingLayoutModelForeign::instance = &tiling;
    DesktopFileIntegrationForeign::instance = &files;
    ApplicationLifecycleModelForeign::instance = &lifecycle;
    TerminalSurfaceControllerForeign::instance = &surfaces;
    DesktopTrayForeign::instance = &tray;
}
}
