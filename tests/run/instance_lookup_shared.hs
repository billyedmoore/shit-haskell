{-# LANGUAGE FlexibleInstances, MultiParamTypeClasses, FunctionalDependencies, UndecidableInstances #-}
-- Instance lookup resolves a wanted's arguments once for all candidates. Matching must
-- still share repeated variables, choose the most specific overlap, and improve fundeps.
module Main where

class Describe a where
  describe :: a -> String

instance {-# OVERLAPPABLE #-} Describe a where
  describe _ = "any"

instance {-# OVERLAPPABLE #-} Describe [a] where
  describe _ = "list"

instance Describe [Int] where
  describe xs = "ints " ++ show (sum xs)

instance {-# OVERLAPPABLE #-} Describe (a, b) where
  describe _ = "pair"

instance Describe (a, a) where
  describe _ = "same pair"

class Unwrap a b | a -> b where
  unwrap :: a -> b

newtype Box a = Box a

instance Unwrap (Box a) a where
  unwrap (Box x) = x

main :: IO ()
main = do
  putStrLn (describe True)
  putStrLn (describe "abc")
  putStrLn (describe (map (+ 1) [1, 2, 3 :: Int]))
  putStrLn (describe (True, 'x'))
  putStrLn (describe (True, False))
  putStrLn (describe (unwrap (Box [4, 5 :: Int])))
  print (sum (unwrap (Box [10, 20 :: Int])))
