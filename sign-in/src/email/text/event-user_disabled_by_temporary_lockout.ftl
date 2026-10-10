<#ftl output_format="plainText">
<#import "../sentences.ftl" as say>
${msg("eventUserDisabledByTemporaryLockoutSubject")}

${msg("kdsTemporaryLockoutText", say.moment())}

${msg("kdsLockoutNote")}
