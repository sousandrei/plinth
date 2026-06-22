-- Drop the existing FTS table and its triggers
DROP TRIGGER IF EXISTS transactions_ai;
DROP TRIGGER IF EXISTS transactions_ad;
DROP TRIGGER IF EXISTS transactions_au;
DROP TABLE IF EXISTS transactions_fts;

-- Recreate transactions_fts with trigram tokenizer, indexing description (text) and amount
CREATE VIRTUAL TABLE transactions_fts USING fts5 (
    id UNINDEXED,
    text,
    amount,
    tokenize      = 'trigram'
);

-- Keep FTS index in sync with the transactions table
CREATE TRIGGER transactions_ai AFTER INSERT ON transactions BEGIN
    INSERT INTO transactions_fts (rowid, id, text, amount)
    VALUES (
        new.rowid,
        new.id,
        new.text,
        CAST(new.amount AS TEXT) || ' ' || printf('%.2f', new.amount / 100.0) || ' ' || printf('%.2f', abs(new.amount) / 100.0)
    );
END;

CREATE TRIGGER transactions_ad AFTER DELETE ON transactions BEGIN
    DELETE FROM transactions_fts WHERE rowid = old.rowid;
END;

CREATE TRIGGER transactions_au AFTER UPDATE ON transactions BEGIN
    DELETE FROM transactions_fts WHERE rowid = old.rowid;
    INSERT INTO transactions_fts (rowid, id, text, amount)
    VALUES (
        new.rowid,
        new.id,
        new.text,
        CAST(new.amount AS TEXT) || ' ' || printf('%.2f', new.amount / 100.0) || ' ' || printf('%.2f', abs(new.amount) / 100.0)
    );
END;

-- Populate the FTS index for existing transactions
INSERT INTO transactions_fts (rowid, id, text, amount)
SELECT 
    rowid,
    id,
    text,
    CAST(amount AS TEXT) || ' ' || printf('%.2f', amount / 100.0) || ' ' || printf('%.2f', abs(amount) / 100.0)
FROM transactions;
