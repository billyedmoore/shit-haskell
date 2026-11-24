{-# LANGUAGE GADTs, NoGADTSyntax #-}

data T where
  Mk :: Int -> T

main :: IO ()
main = pure ()
