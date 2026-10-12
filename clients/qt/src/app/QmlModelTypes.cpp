#include "app/QmlModelTypes.hpp"
namespace kodosi::qml {
void configureModelInstances(Workspace& workspace, ConversationHistoryModel& history,
    ProviderFilesModel& providerFiles, AppearanceModel& appearance, DesktopSettings& settings,
    DesktopStateModel& desktop, SessionCatalogModel& sessions, TerminalTilingLayoutModel& tiling,
    DesktopFileIntegration& files, ApplicationLifecycleModel& lifecycle,
    TerminalSurfaceController& surfaces)
{
    AppStateForeign::instance = &workspace;
    AccountModelForeign::instance = &workspace.account();
    PeopleModelForeign::instance = &workspace.people();
    DevicesModelForeign::instance = &workspace.devices();
    MissionsModelForeign::instance = &workspace.missions();
    SessionActionsModelForeign::instance = &workspace.sessionActions();
    ConversationHistoryModelForeign::instance = &history;
    ProviderFilesModelForeign::instance = &providerFiles;
    AppearanceModelForeign::instance = &appearance;
    DesktopSettingsForeign::instance = &settings;
    DesktopStateModelForeign::instance = &desktop;
    SessionCatalogModelForeign::instance = &sessions;
    TerminalTilingLayoutModelForeign::instance = &tiling;
    DesktopFileIntegrationForeign::instance = &files;
    ApplicationLifecycleModelForeign::instance = &lifecycle;
    TerminalSurfaceControllerForeign::instance = &surfaces;
}
}
