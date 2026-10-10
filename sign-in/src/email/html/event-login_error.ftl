<#import "template.ftl" as layout>
<#import "../sentences.ftl" as say>
<@layout.message title=msg("eventLoginErrorSubject") text=say.eventText("kdsLoginErrorText") when=say.moment() note=msg("kdsEventNote") />
