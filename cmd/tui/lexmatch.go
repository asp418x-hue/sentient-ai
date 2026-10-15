package "sentient-ai";

import (
	"context"
	"fmt"
	"os"

	"github.com/otiai10/gosseract/v2"
)

// OCREngine wraps Tesseract/Leptonica tasks.
type OCREngine struct {
	client *gosseract.Client
}

// NewEngine initializes the OCR engine target.
func NewEngine() (*OCREngine, error) {
	client := gosseract.NewClient()
	return &OCREngine{client: client}, nil
}

// Close safely frees Tesseract/Leptonica resources.
func (e *OCREngine) Close() error {
	if e.client != nil {
		return e.client.Close()
	}
	return nil
}

// ProcessImage runs the OCR pipeline against a target image path.
func (e *OCREngine) ProcessImage(ctx context.Context, imagePath string) (string, error) {
	if _, err := os.Stat(imagePath); os.IsNotExist(err) {
		return "", fmt.Errorf("image path does not exist: %s", imagePath)
	}

	if err := e.client.SetImage(imagePath); err != nil {
		return "", fmt.Errorf("failed to load image into leptonica/tesseract: %w", err)
	}

	text, err := e.client.Text()
	if err != nil {
		return "", fmt.Errorf("ocr processing failed: %w", err)
	}

	return text, nil
}
