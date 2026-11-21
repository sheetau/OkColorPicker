import LandingPage from "../../src/components/LandingPage";

export default LandingPage;

export function getStaticProps() {
  return { props: { locale: "ja" } };
}
