<#import "template.ftl" as layout>
<@layout.message title=msg("emailVerificationSubject") text=msg("kdsVerifyText", realmName) action=msg("kdsVerifyAction") link=link note=msg("kdsVerifyNote", linkExpirationFormatter(linkExpiration)) />
