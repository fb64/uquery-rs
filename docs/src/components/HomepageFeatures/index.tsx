import clsx from 'clsx';
import Heading from '@theme/Heading';
import ThemedImage from '@theme/ThemedImage';
import useBaseUrl from '@docusaurus/useBaseUrl';
import styles from './styles.module.css';

type FeatureItem = {
  title: string;
  // Rendered as <img>: inlining breaks the masks and <use> refs in some logos.
  img: {light: string; dark: string};
  description: JSX.Element;
};

const FeatureList: FeatureItem[] = [
  {
    title: 'Lightweight & Fast',
    img: {light: '/img/uquery.svg', dark: '/img/uquery-dark.svg'},
    description: (
      <>
        µQuery was designed to be deployed on a serverless infrastructure..
      </>
    ),
  },
  {
    title: 'Focus on performances',
    img: {light: '/img/rust-logo-blk.svg', dark: '/img/rust-logo-white.svg'},
    description: (
      <>
        Written in rust language and optimized for performances. Query results are directly streamed back to the client.
      </>
    ),
  },
  {
    title: 'Powered by DuckDB',
    img: {light: '/img/DuckDB_Logo-horizontal.svg', dark: '/img/DuckDB_Logo-horizontal-dark.svg'},
    description: (
      <>
        Embeds DuckDB engine to use rich SQL features to query your data.
      </>
    ),
  },
];

function Feature({title, img, description}: FeatureItem) {
  return (
    <div className={clsx('col col--4')}>
      <div className={styles.featureLogo}>
        <ThemedImage
          className={styles.featureImg}
          alt={title}
          sources={{light: useBaseUrl(img.light), dark: useBaseUrl(img.dark)}}
        />
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
