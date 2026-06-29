export interface User {
  id: string;
  name: string;
  has_pin: boolean;
  created_at: string;
  updated_at: string;
}

export interface Space {
  id: string;
  name: string;
  role: string;
  created_at: string;
  updated_at: string;
}

export interface SpaceMember {
  user_id: string;
  name: string;
  role: string;
}

export interface Account {
  id: string;
  name: string;
  currency: string;
  account_type: string;
  account_source: string;
  color: string;
  space_id: string;
}

export interface Transaction {
  id: string;
  booking_date: string;
  value_date: string;
  reference: string;
  text: string;
  currency: string;
  amount: number;
  balance: number;
  approved: boolean;
  note: string;
  category: string | null;
  account_id: string;
}

export interface Category {
  id: string;
  name: string;
  color: string;
}

export interface TransactionPage {
  transactions: Transaction[];
  page_count: number;
}

export interface ListTransactionsParams {
  page?: number;
  limit?: number;
  search?: string;
  approved?: boolean;
  category?: string;
  dateFrom?: string;
  dateTo?: string;
}

// Aggregations — keyed by "YYYY-MM"
export interface AggregatedMonth {
  by_category: Record<string, number>; // category → sum of amounts (minor units)
  balance: Record<string, number>; // account_id → closing balance (minor units)
}

export type Aggregations = Record<string, AggregatedMonth>;

// ---------------------------------------------------------------------------
// Training / fine-tuning
// ---------------------------------------------------------------------------

export interface FinetuneConfig {
  epochs: number;
  batch_size: number;
  learning_rate: number;
  from_scratch?: boolean;
}

export interface FinetuneProgress {
  epoch: number;
  total_epochs: number;
  train_loss: number;
  train_accuracy: number;
  val_loss: number;
  val_accuracy: number;
}

export interface EmbedProgress {
  current: number;
  total: number;
}

export interface FinetuneResult {
  version: number;
  epochs_completed: number;
  final_train_loss: number;
  final_train_accuracy: number;
  final_val_loss: number;
  final_val_accuracy: number;
  samples_used: number;
  epoch_history: FinetuneProgress[];
}

export interface ModelCard {
  version: number;
  trained_at: string;
  epochs: number;
  samples_used: number;
  train_loss: number | null;
  train_accuracy: number;
  val_loss: number | null;
  val_accuracy: number;
  is_base: boolean;
  is_active: boolean;
  epoch_history: FinetuneProgress[];
}

export interface TrainingSample {
  id: string;
  text: string;
  amount: number;
  booking_date: string;
  actual_category: string;
  predicted_category: string;
}

export interface AccountSummary {
  month: string; // "YYYY-MM"
  account_id: string;
  balance: number; // minor units
}

export interface PeerInfo {
  device_id: string;
  name: string;
  host: string;
  port: number;
  last_seen: number;
}

export interface TrustedDevice {
  id: string;
  space_id: string;
  device_id: string;
  display_name: string;
  paired_at: string;
}

export interface PairToken {
  token: string;
  address: string;
  expires_at_unix: number;
}

export interface JoinResult {
  space_id: string;
  space_name: string;
}

export interface SyncSummary {
  dialed: number;
  ok: number;
  failed: SyncFailure[];
}

export interface SyncFailure {
  device_id: string;
  name: string;
  error: string;
}

// ---------------------------------------------------------------------------
// Space export / import
// ---------------------------------------------------------------------------

export interface ExportResult {
  categories: number;
  accounts: number;
  transactions: number;
  account_summaries: number;
}

export interface ImportResult {
  categories: number;
  accounts: number;
  transactions: number;
  account_summaries: number;
}
