<#import "template.ftl" as layout>
<#import "../sentences.ftl" as say>
<@layout.message title=msg("eventRemoveCredentialSubject") text=say.wayText("kdsRemoveCredentialText", "kdsRemoveWayText") when=say.moment() note=msg("kdsEventNote") />
