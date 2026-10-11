import type { App } from 'vue';
import { installDanQingFeedback } from '@danqing/dq-shell';
import {
  DqAlert,
  DqButton,
  DqCollapse,
  DqCollapseItem,
  DqDrawer,
  DqEmpty,
  DqIcon,
  DqIconButton,
  DqInput,
  DqInputNumber,
  DqOption,
  DqPrefRow,
  DqProgress,
  DqSegmented,
  DqSelect,
  DqSlider,
  DqStack,
  DqSwitch,
  DqTag,
  DqText,
  DqTooltip,
} from '@danqing/dq-shell';

const DQ_COMPONENTS = {
  DqAlert,
  DqButton,
  DqCollapse,
  DqCollapseItem,
  DqDrawer,
  DqEmpty,
  DqIcon,
  DqIconButton,
  DqInput,
  DqInputNumber,
  DqOption,
  DqPrefRow,
  DqProgress,
  DqSegmented,
  DqSelect,
  DqSlider,
  DqStack,
  DqSwitch,
  DqTag,
  DqText,
  DqTooltip,
} as const;

export function installDanQingUi(app: App) {
  installDanQingFeedback(app);
  for (const [name, component] of Object.entries(DQ_COMPONENTS)) {
    app.component(name, component);
  }
}
