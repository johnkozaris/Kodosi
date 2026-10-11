<#import "template.ftl" as layout>
<#import "../sentences.ftl" as say>
<@layout.message title=msg("eventUserDisabledByTemporaryLockoutSubject") text=msg("kdsTemporaryLockoutText", say.moment()) when=say.moment() note=msg("kdsLockoutNote") />
