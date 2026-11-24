{-# LANGUAGE Haskell2010, DoRec #-}

import Control.Monad.Fix ()

main :: IO ()
main = mdo
  x <- pure (1 :: Int)
  print x
