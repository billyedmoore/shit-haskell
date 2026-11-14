{-# LANGUAGE RankNTypes #-}
module Main where

type Closed = ([Int], Maybe Bool, String)
data Packet a = Packet Closed a deriving (Eq, Show)

closed :: Closed
closed = ([1, 2], Just True, "fixed")

make :: a -> Packet a
make = Packet closed

both :: (forall a. a -> a) -> (Packet Int, Packet Bool)
both f = (f (make 7), f (make True))

identity :: a -> a
identity x = x

main :: IO ()
main = do
  print (both identity)
  let left = identity (make 'x')
      right = identity (make (Just (9 :: Int)))
  print (left, right)
  print (identity closed == closed)
