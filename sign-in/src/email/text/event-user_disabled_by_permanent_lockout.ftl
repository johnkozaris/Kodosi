<#ftl output_format="plainText">
<#import "../sentences.ftl" as say>
${msg("eventUserDisabledByPermanentLockoutSubject")}

${msg("kdsPermanentLockoutText", say.moment())}
