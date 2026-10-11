<#import "template.ftl" as layout>
<#import "../sentences.ftl" as say>
<@layout.message title=msg("kdsLinkTitle") text=say.linkText() action=msg("kdsLinkAction") link=link note=msg("kdsLinkNote", linkExpirationFormatter(linkExpiration)) />
