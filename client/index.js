const watchFreqSliders = () => {
  const MIN_SLIDER_RANGE = 30;

  let minSlider = document.getElementById("min-freq-input");
  let maxSlider = document.getElementById("max-freq-input");
  let minValue = document.getElementById("min-freq-value");
  let maxValue = document.getElementById("max-freq-value");

  if (!minSlider || !maxSlider || !minValue || !maxValue) {
    return;
  }

  minSlider.addEventListener("change", (event) => {
    maxSlider.value = event.target.value + MIN_SLIDER_RANGE;
    minValue.textContent = event.target.value;
  });

  maxSlider.addEventListener("change", (event) => {
    minSlider.value = event.target.value - MIN_SLIDER_RANGE;
    maxValue.textContent = event.target.value;
  });
};

watchFreqSliders();
