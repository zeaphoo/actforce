import tailwindcss from 'tailwindcss'
import autoprefixer from 'autoprefixer'
import postcssPresetEnv from 'postcss-preset-env'

export default {
  plugins: {
    tailwindcss: {},
    autoprefixer: {},
    'postcss-preset-env': {
      stage: 0,
      browsers: 'Chrome >= 68',
    },
  },
}
