package lexmatch

import (
		"context"
			"fmt"
				"os"

					"github.com/otiai10/gosseract/v2"
				)

				type OCREngine struct {
						client *gosseract.Client
					}

					func NewEngine() (*OCREngine, error) {
							client := gosseract.NewClient()
								return &OCREngine{client: client}, nil
							}

							func (e *OCREngine) Close() error {
									if e.client != nil {
												return e.client.Close()
													}
														return nil
													}

													func (e *OCREngine) ProcessImage(ctx context.Context, imagePath string) (string, error) {
															if _, err := os.Stat(imagePath); os.IsNotExist(err) {
																		return "", fmt.Errorf("file not found: %s", imagePath)
																			}

																				if err := e.client.SetImage(imagePath); err != nil {
																							return "", fmt.Errorf("failed to load image: %w", err)
																								}

																									text, err := e.client.Text()
																										if err != nil {
																													return "", fmt.Errorf("ocr extraction failed: %w", err)
																														}

																															return text, nil
																														}

