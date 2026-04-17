export type { MandoAPI } from '#preload/types/api';

import { exposeApi } from '#preload/ipc/expose';

exposeApi();
