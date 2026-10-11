<#import "template.ftl" as layout>
<#import "../sentences.ftl" as say>
<@layout.message title=msg("eventUserDisabledByPermanentLockoutSubject") text=msg("kdsPermanentLockoutText", say.moment()) when=say.moment() />
