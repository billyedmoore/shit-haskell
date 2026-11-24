{-# LANGUAGE DatatypeContexts, GADTs #-}

data Eq a => T a where
  Mk :: a -> T a

main :: IO ()
main = pure ()
