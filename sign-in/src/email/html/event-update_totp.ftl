<#import "template.ftl" as layout>
<#import "../sentences.ftl" as say>
<@layout.message title=msg("eventUpdateTotpSubject") text=say.eventText("kdsUpdateTotpText") when=say.moment() note=msg("kdsEventNote") />
