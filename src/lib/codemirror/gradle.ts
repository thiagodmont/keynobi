import { StreamLanguage, LanguageSupport } from "@codemirror/language";
import { clike } from "@codemirror/legacy-modes/mode/clike";

const gradleDslKeywords = [
  "fun", "val", "var", "class", "interface", "object", "when", "if", "else",
  "for", "while", "return", "import", "package", "true", "false", "null",
  // Gradle DSL
  "plugins", "dependencies", "repositories", "android", "buildTypes",
  "productFlavors", "defaultConfig", "sourceSets", "configurations",
  "implementation", "testImplementation", "androidTestImplementation",
  "api", "compileOnly", "runtimeOnly", "kapt", "ksp", "annotationProcessor",
  "compileSdk", "minSdk", "targetSdk", "versionCode", "versionName",
  "applicationId", "namespace", "buildFeatures", "composeOptions",
  "signingConfigs", "release", "debug", "minifyEnabled", "proguardFiles",
  "flavorDimensions", "dimension", "resValue", "buildConfigField",
  "kotlin", "id", "version", "apply", "allprojects", "subprojects",
];

const gradleMode = clike({
  name: "gradle",
  keywords: gradleDslKeywords.reduce((acc: Record<string, string>, kw) => {
    acc[kw] = "keyword";
    return acc;
  }, {}),
  atoms: { true: "atom", false: "atom", null: "atom" },
  multiLineStrings: true,
});

export const gradleLanguage = StreamLanguage.define(gradleMode);

export function gradle(): LanguageSupport {
  return new LanguageSupport(gradleLanguage);
}
