<#import "template.ftl" as layout>
<@layout.message title=msg("emailUpdateConfirmationSubject") text=msg("kdsNewEmailText", realmName, newEmail) action=msg("kdsNewEmailAction") link=link note=msg("kdsNewEmailNote", linkExpirationFormatter(linkExpiration)) />
