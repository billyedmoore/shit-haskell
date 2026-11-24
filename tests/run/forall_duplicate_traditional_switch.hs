{-# LANGUAGE ExistentialQuantification, NoExplicitForAll #-}

data D = forall a. D a

main :: IO ()
main = pure ()
