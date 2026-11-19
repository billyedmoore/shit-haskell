-- A default signature's own context solved from the method's (basement's
-- `scale :: IsNatural n => n -> a -> a` with `default scale :: (Enum n, IsNatural n) => ..`:
-- `Enum n` is a superclass of `IsNatural n`, through `Integral`).
{-# LANGUAGE DefaultSignatures #-}

class (Integral a, Eq a, Ord a) => IsIntegral a
class IsIntegral a => IsNatural a
instance IsIntegral Int
instance IsNatural Int
instance IsIntegral Integer
instance IsNatural Integer

class Additive a where
  azero :: a
  plus :: a -> a -> a
  scale :: IsNatural n => n -> a -> a
  default scale :: (Enum n, IsNatural n) => n -> a -> a
  scale = scaleEnum

scaleEnum :: (Enum n, IsNatural n, Additive a) => n -> a -> a
scaleEnum 0 _ = azero
scaleEnum 1 a = a
scaleEnum n a = plus a (scaleEnum (pred n) a)

instance Additive [x] where
  azero = []
  plus = (++)

newtype Sum' = Sum' Int deriving Show
instance Additive Sum' where
  azero = Sum' 0
  plus (Sum' a) (Sum' b) = Sum' (a + b)
  scale n (Sum' a) = Sum' (fromIntegral n * a)

main :: IO ()
main = do
  putStrLn (scale (3 :: Int) "ab")
  print (scale (4 :: Integer) [True])
  print (scale (5 :: Int) (Sum' 7))
