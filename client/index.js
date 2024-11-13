let freqSliderEventsRegistered = false;

// Register event listeners for the squirt frequency sliders.
const watchFreqSliders = () => {
  // The min and max sliders must always be at least MIN_SLIDER_RANGE apart.
  const MIN_SLIDER_RANGE = 10;

  if (freqSliderEventsRegistered) {
    return;
  }

  let minSlider = document.getElementById("min-freq-input");
  let maxSlider = document.getElementById("max-freq-input");
  let minValue = document.getElementById("min-freq-value");
  let maxValue = document.getElementById("max-freq-value");

  if (!minSlider || !maxSlider || !minValue || !maxValue) {
    return;
  }

  // We don't want to register the event listeners until the actual input
  // elements (which will not be disabled) are loaded from the server via HTMX.
  // The disabled inputs are placeholders until that request completes.
  if (minSlider.disabled || maxSlider.disabled) {
    return;
  }

  // In practice, the min and max values will always be the same for both
  // sliders.
  const originalUpperBound = parseInt(maxSlider.max);
  const originalLowerBound = parseInt(minSlider.min);

  minSlider.addEventListener("input", (event) => {
    // Ensure the min slider is always at least MIN_SLIDER_RANGE below the max
    // slider. This comes up when the max slider is at its upper bound.
    if (parseInt(maxSlider.value) === originalUpperBound) {
      event.target.value = Math.min(
        event.target.value,
        originalUpperBound - MIN_SLIDER_RANGE
      );
    }

    // Ensure the max slider is always at least MIN_SLIDER_RANGE above the min
    // slider.
    maxSlider.value = Math.max(
      parseInt(maxSlider.value),
      parseInt(event.target.value) + MIN_SLIDER_RANGE
    );

    // Update the displayed values match the sliders.
    minValue.textContent = event.target.value;
    maxValue.textContent = maxSlider.value;
  });

  maxSlider.addEventListener("input", (event) => {
    // Ensure the max slider is always at least MIN_SLIDER_RANGE above the min
    // slider. This comes up when the min slider is at its lower bound.
    if (parseInt(minSlider.value) === originalLowerBound) {
      event.target.value = Math.max(event.target.value, MIN_SLIDER_RANGE);
    }

    // Ensure the min slider is always at least MIN_SLIDER_RANGE below the max
    // slider.
    minSlider.value = Math.min(
      parseInt(minSlider.value),
      parseInt(event.target.value) - MIN_SLIDER_RANGE
    );

    // Update the displayed values match the sliders.
    maxValue.textContent = event.target.value;
    minValue.textContent = minSlider.value;
  });

  freqSliderEventsRegistered = true;
};
