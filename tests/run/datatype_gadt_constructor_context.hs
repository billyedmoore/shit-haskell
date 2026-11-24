{-# LANGUAGE NoDatatypeContexts, GADTs #-}

data T a where
  Mk :: Eq a => a -> T a

main :: IO ()
main = case Mk (4 :: Int) of Mk x -> print x
