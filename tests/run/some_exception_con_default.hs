-- The SomeException constructor used as a function with no exception context in scope (tar's
-- unpack): GHC defaults ?exceptionContext to the empty context.
{-# LANGUAGE RankNTypes #-}
import Control.Exception

check :: Int -> Maybe ArithException
check n = if n < 0 then Just Underflow else Nothing

run :: (Int -> Maybe SomeException) -> Int -> String
run f n = maybe "ok" displayException (f n)

go :: Int -> String
go = run (fmap SomeException . check)

main = mapM_ (putStrLn . go) [1, -1]
