<#import "template.ftl" as layout>
<#import "../sentences.ftl" as say>
<@layout.message title=msg("eventUpdateCredentialSubject") text=say.wayText("kdsUpdateCredentialText", "kdsUpdateWayText") when=say.moment() note=msg("kdsEventNote") />
