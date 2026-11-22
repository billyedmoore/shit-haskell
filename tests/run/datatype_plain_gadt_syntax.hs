{-# LANGUAGE NoExistentialQuantification, NoGADTs, GADTSyntax #-}

data T a where
  Mk :: forall b. b -> T b

main :: IO ()
main = case Mk (4 :: Int) of Mk x -> print x
