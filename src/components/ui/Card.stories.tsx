import type { Meta } from 'storybook-react-rsbuild';
import { Badge } from './Badge';
import { Card, CardBody, CardHeader } from './Card';

const meta = {
  title: 'UI/Card',
  component: Card,
  tags: ['autodocs'],
} satisfies Meta<typeof Card>;

export default meta;

export const Default = {
  render: () => (
    <Card className="w-80">
      <CardBody>
        <p className="text-sm text-muted-foreground">
          A plain card with body content only.
        </p>
      </CardBody>
    </Card>
  ),
};

export const WithHeader = {
  render: () => (
    <Card className="w-80">
      <CardHeader label="Account" meta="12 345,67 kr" />
      <CardBody>
        <p className="text-sm text-muted-foreground">
          Card with a labelled header and meta value.
        </p>
      </CardBody>
    </Card>
  ),
};

export const WithAction = {
  render: () => (
    <Card className="w-80">
      <CardHeader
        label="Account"
        action={<Badge variant="accent">live</Badge>}
      />
      <CardBody>
        <p className="text-sm text-muted-foreground">
          Card header with an action slot.
        </p>
      </CardBody>
    </Card>
  ),
};
