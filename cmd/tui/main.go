package "sentient-ai";

import (
		"context"
			"fmt"
				"os"

					tea "github.com/charmbracelet/bubbletea"
						"github.com/charmbracelet/lipgloss"
							"sentient-ai/lexmatch"
						)

						type state int

						const (
								stateIdle state = iota
									stateProcessing
										stateSuccess
											stateError
										)

										type model struct {
												state     state
													imagePath string
														result    string
															err       error
																engine    *lexmatch.OCREngine
															}

															// Messages
															type ocrResultMsg struct {
																	text string
																		err  error
																	}

																	func initialModel(imagePath string) (*model, error) {
																			engine, err := lexmatch.NewEngine()
																				if err != nil {
																							return nil, err
																								}
																									return &model{
																												state:     stateIdle,
																														imagePath: imagePath,
																																engine:    engine,
																																	}, nil
																																}

																																func (m *model) Init() tea.Cmd {
																																		return nil
																																	}

																																	func (m *model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
																																			switch msg := msg.(type) {
																																					case tea.KeyMsg:
																																								switch msg.String():
																																										case "q", "ctrl+c":
																																														m.engine.Close()
																																																	return m, tea.Quit
																																																			case "enter":
																																																							if m.state == stateIdle || m.state == stateError {
																																																												m.state = stateProcessing
																																																																return m, runOCR(m.engine, m.imagePath)
																																																																			}
																																																																					}

																																																																						case ocrResultMsg:
																																																																									if msg.err != nil {
																																																																													m.state = stateError
																																																																																m.err = msg.err
																																																																																		} else {
																																																																																						m.state = stateSuccess
																																																																																									m.result = msg.text
																																																																																											}
																																																																																												}
																																																																																													return m, nil
																																																																																												}

																																																																																												func (m *model) View() string {
																																																																																														var s string
																																																																																															switch m.state {
																																																																																																	case stateIdle:
																																																																																																				s = fmt.Sprintf("Ready to process target: %s\n\nPress [Enter] to start OCR or [q] to quit.", m.imagePath)
																																																																																																					case stateProcessing:
																																																																																																								s = "Running Leptonica preprocessing & Tesseract analysis..."
																																																																																																									case stateSuccess:
																																																																																																												s = fmt.Sprintf("OCR Extracted Text:\n\n%s\n\nPress [q] to exit.", m.result)
																																																																																																													case stateError:
																																																																																																																s = fmt.Sprintf("Error: %v\n\nPress [Enter] to retry or [q] to quit.", m.err)
																																																																																																																	}
																																																																																																																		return lipgloss.NewStyle().Padding(2).Render(s)
																																																																																																																	}

																																																																																																																	func runOCR(engine *lexmatch.OCREngine, path string) tea.Cmd {
																																																																																																																			return func() tea.Msg {
																																																																																																																						ctx := context.Background()
																																																																																																																								text, err := engine.ProcessImage(ctx, path)
																																																																																																																										return ocrResultMsg{text: text, err: err}
																																																																																																																											}
																																																																																																																										}

																																																																																																																										func main() {
																																																																																																																												if len(os.Args) < 2 {
																																																																																																																															fmt.Println("Usage: tui <path-to-image>")
																																																																																																																																	os.Exit(1)
																																																																																																																																		}

																																																																																																																																			m, err := initialModel(os.Args[1])
																																																																																																																																				if err != nil {
																																																																																																																																							fmt.Printf("Initialization error: %v\n", err)
																																																																																																																																									os.Exit(1)
																																																																																																																																										}

																																																																																																																																											p := tea.NewProgram(m)
																																																																																																																																												if _, err := p.Run(); err != nil {
																																																																																																																																															fmt.Printf("Error running TUI: %v\n", err)
																																																																																																																																																	os.Exit(1)
																																																																																																																																																		}
																																																																																																																																																	}

