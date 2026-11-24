{-# LANGUAGE DatatypeContexts, NoExistentialQuantification, NoGADTs #-}

data Eq a => T a = Mk a

main :: IO ()
main = case Mk (4 :: Int) of Mk x -> print x
