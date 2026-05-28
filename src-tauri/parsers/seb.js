function transformSEB(data, unitName, unitAccountType) {
  var rows = data.rows;

  // Search first 10 rows for account number inside parens: e.g. "Konto (5000 1234567)"
  var accountNumber = null;
  for (var i = 0; i < Math.min(10, rows.length); i++) {
    var row = rows[i];
    for (var key in row) {
      var match = String(row[key]).match(/\(([\d\s]+)\)/);
      if (match) {
        accountNumber = match[1].replace(/\s+/g, '');
        break;
      }
    }
    if (accountNumber) break;
  }

  if (!accountNumber) {
    throw new Error(`${unitName}: could not extract account number from xlsx`);
  }

  var accountId = md5(accountNumber);
  var transactions = [];

  for (var j = 0; j < rows.length; j++) {
    var r = rows[j];

    var bookingDate =
      r.Bokföringsdag || r.Bokforingsdag || r['Booking date'] || '';
    var valueDate = r.Valutadag || r['Value date'] || bookingDate;
    var reference =
      r.Verifikationsnummer || r['Voucher number'] || r.Reference || '';
    var text = r.Text || '';
    var amountStr = r.Belopp || r.Amount || '0';
    var balanceStr = r.Saldo || r.Balance || '0';

    if (!bookingDate || !text) continue;

    // Standardise number string parsing to support Sweden/English separator formats
    var amount = Math.trunc(
      parseFloat(String(amountStr).replace(/\s/g, '').replace(',', '.')) * 100,
    );
    var balance = Math.trunc(
      parseFloat(String(balanceStr).replace(/\s/g, '').replace(',', '.')) * 100,
    );

    if (Number.isNaN(amount) || Number.isNaN(balance)) continue;

    var id = md5(valueDate + reference + text + amount + balance);

    transactions.push({
      id: id,
      booking_date: bookingDate,
      value_date: valueDate,
      reference: reference,
      text: text,
      amount: amount,
      balance: balance,
    });
  }

  return {
    type: unitAccountType,
    account_id: accountId,
    transactions: transactions,
  };
}

export default {
  bank: 'SEB',
  units: [
    {
      key: 'seb_checking',
      name: 'SEB Checking',
      format: 'xlsx',
      account_type: 'checking',
      account_source: 'seb',
      currency: 'SEK',
      transform(data) {
        return transformSEB(data, 'seb_checking', 'checking');
      },
    },
    {
      key: 'seb_savings',
      name: 'SEB Savings',
      format: 'xlsx',
      account_type: 'savings',
      account_source: 'seb',
      currency: 'SEK',
      transform(data) {
        return transformSEB(data, 'seb_savings', 'savings');
      },
    },
  ],
};
