\documentclass{article}
\begin{document}
A LaTeX-style literate module: the code is between begin and end markers.
\begin{code}
module Latex (twice) where

twice :: Int -> Int
twice x = x + x
\end{code}
Commentary after the code.
\end{document}
