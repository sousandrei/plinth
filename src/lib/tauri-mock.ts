const listCommands = new Set([
  'list_accounts',
  'list_account_summaries',
  'list_transactions',
  'list_all_categories',
  'list_users',
  'list_my_spaces',
  'list_space_members',
  'list_peers',
  'list_trusted_devices',
  'list_models',
  'list_parsers',
  'list_parser_files',
  'list_training_samples',
]);

const booleanCommands = new Set(['is_classifier_ready', 'minilm_status']);

const numberCommands = new Set([
  'count_approved_transactions',
  'get_training_progress',
]);

const defaultMocks: Record<string, unknown> = {
  get_session: null,
  get_app_setting: null,
  get_device_name: 'Storybook',
  get_local_address: '127.0.0.1',
  get_training_device: 'cpu',
};

function defaultForCommand(command: string): unknown {
  if (defaultMocks[command] !== undefined) return defaultMocks[command];
  if (listCommands.has(command)) return [];
  if (booleanCommands.has(command)) return false;
  if (numberCommands.has(command)) return 0;
  if (command.startsWith('get_')) return null;
  if (command.startsWith('count_')) return 0;
  if (command.startsWith('is_')) return false;
  return null;
}

type MockValue = unknown | (() => unknown);

const storyMocks: Record<string, MockValue> = {};

export function setMock(command: string, value: MockValue): void {
  storyMocks[command] = value;
}

export function clearMocks(): void {
  for (const key of Object.keys(storyMocks)) {
    delete storyMocks[key];
  }
}

export async function invoke<T = unknown>(
  command: string,
  _args?: Record<string, unknown> | unknown[],
): Promise<T> {
  if (command in storyMocks) {
    const mock = storyMocks[command];
    return (typeof mock === 'function' ? mock() : mock) as T;
  }
  return defaultForCommand(command) as T;
}
