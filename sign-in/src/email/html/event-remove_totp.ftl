<#import "template.ftl" as layout>
<#import "../sentences.ftl" as say>
<@layout.message title=msg("eventRemoveTotpSubject") text=say.eventText("kdsRemoveTotpText") when=say.moment() note=msg("kdsEventNote") />
