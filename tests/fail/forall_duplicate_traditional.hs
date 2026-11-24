{-# LANGUAGE ExistentialQuantification, NoExplicitForAll #-}

data D = forall a a. D a

main :: IO ()
main = pure ()
