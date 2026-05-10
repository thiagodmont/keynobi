import type { Meta, StoryObj } from "storybook-solidjs-vite";
import { Alert, Badge, Button, EmptyState, ProgressBar, Spinner, StatusDot } from "@/components/ui";
import "./design-system.stories.css";

const meta = {
  title: "Design System/Feedback",
  tags: ["autodocs"],
  parameters: {
    docs: {
      description: {
        component:
          "Feedback primitives for state, progress, empty views, and compact semantic markers.",
      },
    },
  },
} satisfies Meta;

export default meta;
type Story = StoryObj<typeof meta>;

export const StatusAndProgress: Story = {
  render: () => (
    <div class="dsPage">
      <section class="dsSection">
        <h2 class="dsSectionTitle">Status Dots and Badges</h2>
        <div class="dsStack">
          <StatusDot status="ok" />
          <StatusDot status="warning" />
          <StatusDot status="error" />
          <StatusDot status="active" />
          <StatusDot status="idle" />
          <Badge variant="success" dot>
            Connected
          </Badge>
          <Badge variant="warning" dot>
            Degraded
          </Badge>
          <Badge variant="error" dot>
            Failed
          </Badge>
        </div>
      </section>

      <section class="dsSection">
        <h2 class="dsSectionTitle">Progress</h2>
        <div class="dsGrid">
          <div class="dsCard">
            <div class="dsCardTitle">Determinate</div>
            <ProgressBar value={64} />
            <ProgressBar value={42} variant="success" size="md" />
            <ProgressBar value={20} variant="warning" />
          </div>
          <div class="dsCard">
            <div class="dsCardTitle">Indeterminate</div>
            <ProgressBar />
            <div class="dsStack">
              <Spinner size="sm" />
              <Spinner />
            </div>
          </div>
        </div>
      </section>
    </div>
  ),
};

export const AlertsAndEmptyStates: Story = {
  render: () => (
    <div class="dsPage">
      <div class="dsGrid">
        <Alert
          variant="info"
          title="Device ready"
          action={
            <Button variant="outline" size="xs">
              Refresh
            </Button>
          }
        >
          A physical device is connected and ready for deploy.
        </Alert>
        <Alert variant="warning" title="SDK path missing" dismissible onDismiss={() => {}}>
          Configure Android SDK before running builds.
        </Alert>
        <Alert variant="error" title="Build failed">
          Gradle returned a non-zero exit code.
        </Alert>
        <Alert variant="success" title="Installed">
          The app was installed and launched successfully.
        </Alert>
      </div>

      <EmptyState
        icon="terminal"
        title="No logs yet"
        description="Start Logcat to stream entries from the selected device."
        action={
          <Button variant="outline" size="sm">
            Start
          </Button>
        }
      />
    </div>
  ),
};
