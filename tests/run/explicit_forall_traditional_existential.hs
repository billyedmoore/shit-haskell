{-# LANGUAGE ExistentialQuantification, NoExplicitForAll #-}

data T = forall a. Show a => Mk a

main :: IO ()
main = case Mk (7 :: Int) of Mk x -> print x
