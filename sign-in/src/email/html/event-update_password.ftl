<#import "template.ftl" as layout>
<#import "../sentences.ftl" as say>
<@layout.message title=msg("eventUpdatePasswordSubject") text=say.eventText("kdsUpdatePasswordText") when=say.moment() note=msg("kdsEventNote") />
