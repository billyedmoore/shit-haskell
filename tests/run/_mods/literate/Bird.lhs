/-----------------------------------------------------------------------------
A Bird-style literate module (as happy's sources are): only the lines that
start with `>` are code.
-----------------------------------------------------------------------------

> module Bird (area, Shape (..)) where

> data Shape = Square Double | Rect Double Double
>   deriving Show

Text between code blocks is commentary.

> area :: Shape -> Double
> area (Square s) = s * s
> area (Rect w h) =
>   w * h
