export default {
  bank: 'Avanza',
  units: [
    {
      key: 'avanza_isk',
      name: 'Avanza ISK',
      format: 'pdf',
      account_type: 'investment',
      account_source: 'avanza',
      currency: 'SEK',
      transform(data) {
        var text = data.text;

        if (!text) {
          throw new Error('avanza_isk: no text content extracted from PDF');
        }

        // Extract account number via: KontoKontotyp<digits><type>Kundnamn
        var accountMatch = text.match(/Konto\s*Kontotyp\s*(\d+)/);
        if (!accountMatch) {
          throw new Error(
            'avanza_isk: could not extract account number from PDF text',
          );
        }
        var accountNumber = accountMatch[1].trim();
        var accountId = md5(accountNumber);

        // Extract total portfolio value.
        // Pattern from Go source: Totalt[\d\s]+,\d{2}([\d\s,]+)Ranta
        // The balance follows "Totalt" and a first value (spaces+comma+2dec), then our target.
        var balanceMatch = text.match(/Totalt[\d\s]+,\d{2}([\d\s,]+)R/);
        if (!balanceMatch) {
          throw new Error(
            'avanza_isk: could not extract balance from PDF text',
          );
        }
        var balanceStr = balanceMatch[1].replace(/\s+/g, '').replace(/,/, '.');
        var balance = Math.trunc(parseFloat(balanceStr) * 100);

        if (Number.isNaN(balance)) {
          throw new Error(
            `avanza_isk: balance value is not a number: ${balanceMatch[1]}`,
          );
        }

        // Extract month: Värde <YYYY-MM>-DD Värdepappersinnehav
        var monthMatch = text.match(/V[äa]rde\s+([\d]{4}-[\d]{2})-[\d]{2}/);
        if (!monthMatch) {
          throw new Error('avanza_isk: could not extract month from PDF text');
        }
        var month = monthMatch[1]; // "YYYY-MM"

        return {
          type: 'investment',
          account_id: accountId,
          month: month,
          balance: balance,
        };
      },
    },
  ],
};
