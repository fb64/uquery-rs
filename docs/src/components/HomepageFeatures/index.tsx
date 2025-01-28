import clsx from 'clsx';
import Heading from '@theme/Heading';
import styles from './styles.module.css';

type FeatureItem = {
  title: string;
  Svg: React.ComponentType<React.ComponentProps<'svg'>>;
  description: JSX.Element;
};

const FeatureList: FeatureItem[] = [
  {
    title: 'Lightweight & Fast',
    Svg: require('@site/static/img/uquery.svg').default,
    description: (
      <>
        µQuery was designed to be deployed on a serverless infrastructure..
      </>
    ),
  },
  {
    title: 'Focus on performances',
    Svg: require('@site/static/img/rust-logo-blk.svg').default,
    description: (
      <>
        Written in rust language and optimized for performances. Query results are directly streamed back to the client.
      </>
    ),
  },
  {
    title: 'Powered by DuckDB',
    Svg: require('@site/static/img/DuckDB_Logo-horizontal.svg').default,
    description: (
      <>
        Embeds DuckDB engine to use rich SQL features to query your data.
      </>
    ),
  },
];

function Feature({title, Svg, description}: FeatureItem) {
  return (
    <div className={clsx('col col--4')}>
      <div className="text--center">
        <Svg className={styles.featureSvg} role="img" />
      </div>
      <div className="text--center padding-horiz--md">
        <Heading as="h3">{title}</Heading>
        <p>{description}</p>
      </div>
    </div>
  );
}

export default function HomepageFeatures(): JSX.Element {
  return (
    <section className={styles.features}>
      <div className="container">
        <div className="row">
          {FeatureList.map((props, idx) => (
            <Feature key={idx} {...props} />
          ))}
        </div>
      </div>
    </section>
  );
}
