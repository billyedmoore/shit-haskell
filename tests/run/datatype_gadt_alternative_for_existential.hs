{-# LANGUAGE NoExistentialQuantification, GADTs #-}

data T = forall a. Show a => Mk a

main :: IO ()
main = case Mk (4 :: Int) of Mk x -> print x
