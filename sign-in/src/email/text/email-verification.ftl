<#ftl output_format="plainText">
${msg("emailVerificationSubject")}

${msg("kdsVerifyText", realmName)}

${link}

${msg("kdsVerifyNote", linkExpirationFormatter(linkExpiration))}
