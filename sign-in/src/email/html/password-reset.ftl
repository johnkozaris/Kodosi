<#import "template.ftl" as layout>
<@layout.message title=msg("passwordResetSubject") text=msg("kdsResetText", realmName) action=msg("kdsResetAction") link=link note=msg("kdsResetNote", linkExpirationFormatter(linkExpiration)) />
