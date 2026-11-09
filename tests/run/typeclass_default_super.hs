class Shape a where
  area :: a -> Double
  perimeter :: a -> Double
  summary :: a -> String
  summary x = "area=" ++ show (area x) ++ " perim=" ++ show (perimeter x)

class Shape a => Solid a where
  volume :: a -> Double -> Double
  volume x h = area x * h

data Square = Square Double

instance Shape Square where
  area (Square s) = s * s
  perimeter (Square s) = 4 * s

instance Solid Square

main :: IO ()
main = do
  putStrLn (summary (Square 2))
  print (volume (Square 3) 2)
