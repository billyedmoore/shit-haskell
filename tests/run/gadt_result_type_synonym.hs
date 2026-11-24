{-# LANGUAGE GADTs #-}

type Alias a = T a
data T a where
  Mk :: a -> Alias a

main :: IO ()
main = case Mk (4 :: Int) of Mk x -> print x
