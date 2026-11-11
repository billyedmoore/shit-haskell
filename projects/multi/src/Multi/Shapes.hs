module Multi.Shapes (Shape (..), area, classify) where

data Shape = Circle Double | Rect Double Double
  deriving (Show, Eq)

area :: Shape -> Double
area = \case
  Circle r -> pi * r * r
  Rect w h -> w * h

classify :: Shape -> String
classify s
  | area s > 10 = "big"
  | otherwise = "small"
