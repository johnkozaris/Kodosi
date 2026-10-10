<#import "template.ftl" as layout>
<#import "../sentences.ftl" as say>
<@layout.message title=msg("executeActionsSubject") text=say.setupText() action=msg("kdsSetupAction") link=link note=msg("kdsSetupNote", linkExpirationFormatter(linkExpiration)) />
