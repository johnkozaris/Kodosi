<#ftl output_format="plainText">
${msg("emailUpdateConfirmationSubject")}

${msg("kdsNewEmailText", realmName, newEmail)}

${link}

${msg("kdsNewEmailNote", linkExpirationFormatter(linkExpiration))}
