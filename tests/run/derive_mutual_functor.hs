{-# LANGUAGE DeriveFunctor #-}

newtype ML m a = ML (m (MLView m a)) deriving Functor
data MLView m a = EmptyML | ConsML a (ML m a) deriving Functor

toListML :: ML Maybe a -> [a]
toListML (ML Nothing) = []
toListML (ML (Just EmptyML)) = []
toListML (ML (Just (ConsML a rest))) = a : toListML rest

main :: IO ()
main = do
  let x :: ML Maybe Int
      x = ML (Just (ConsML 1 (ML (Just (ConsML 2 (ML (Just EmptyML)))))))
  print (toListML (fmap (* 10) x))
